use std::cmp::Ordering;

use oxc::{
	allocator::{Allocator, String, Vec},
	ast::ast::AssignmentOperator,
	span::{format_compact_str, Atom, Span},
};
use smallvec::{smallvec, SmallVec};

use crate::{cfg::Config, changeset::ChangeSet, RewriterError};

// const STRICTCHECKER: &str = "(function(a){arguments[0]=false;return a})(true)";
const STRICTCHECKER: &str = "(function(){return !this;})()";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rewrite<'data> {
	/// `(cfg.wrapfn(ident,strictchecker))` | `cfg.wrapfn(ident,strictchecker)`
	WrapFn {
		span: Span,
		wrapped: bool,
	},
	/// `cfg.setrealmfn({}).ident`
	SetRealmFn {
		span: Span,
	},
	/// `cfg.wrapthis(this)`
	WrapThisFn {
		span: Span,
	},
	/// `(cfg.importfn("cfg.base"))`
	ImportFn {
		span: Span,
	},
	/// `cfg.metafn("cfg.base")`
	MetaFn {
		span: Span,
	},

	// dead code only if debug is disabled
	#[allow(dead_code)]
	/// `$scramerr(name)`
	ScramErr {
		span: Span,
		ident: Atom<'data>,
	},
	/// `$scramitize(span)`
	Scramitize {
		span: Span,
	},

	/// `eval(cfg.rewritefn(inner))`
	Eval {
		span: Span,
		inner: Span,
	},
	/// `((t)=>$scramjet$tryset(name,"op",t)||(name op t))(rhsspan)`
	Assignment {
		name: Atom<'data>,
		entirespan: Span,
		rhsspan: Span,
		op: AssignmentOperator,
	},
	/// `ident,` -> `ident: cfg.wrapfn(ident),`
	ShorthandObj {
		span: Span,
		name: Atom<'data>,
	},
	SourceTag {
		span: Span,
	},

	// don't use for anything static, only use for stuff like rewriteurl
	Replace {
		span: Span,
		text: String<'data>,
	},
	Delete {
		span: Span,
	},
	GlobalFn {
		span: Span,
	},
	WindowMemberFn {
		span: Span,
		property_name: Atom<'data>,
	},
}

impl<'data> Rewrite<'data> {
	fn into_inner(self) -> SmallVec<[JsChange<'data>; 4]> {
		match self {
			Self::WrapFn {
				wrapped: extra,
				span,
			} => {
				let start = Span::new(span.start, span.start);
				let end = Span::new(span.end, span.end);

				smallvec![
					JsChange::WrapFnLeft { span: start, extra },
					JsChange::WrapFnRight { span: end, extra }
				]
			}
			Self::SetRealmFn { span } => smallvec![JsChange::SetRealmFn { span }],
			Self::WrapThisFn { span } => smallvec![
				JsChange::WrapThisFn {
					span: Span::new(span.start, span.start)
				},
				JsChange::ClosingParen {
					span: Span::new(span.end, span.end),
					semi: false,
				}
			],
			Self::ImportFn { span } => smallvec![JsChange::ImportFn { span }],
			Self::MetaFn { span } => smallvec![JsChange::MetaFn { span }],

			Self::ScramErr { span, ident } => {
				smallvec![JsChange::ScramErrFn {
					span: Span::new(span.start, span.start),
					ident,
				},]
			}
			Self::Scramitize { span } => {
				smallvec![
					JsChange::ScramitizeFn {
						span: Span::new(span.start, span.start)
					},
					JsChange::ClosingParen {
						span: Span::new(span.end, span.end),
						semi: false,
					}
				]
			}

			Self::Eval { inner, span } => smallvec![
				JsChange::EvalRewriteFn {
					span: Span::new(span.start, inner.start)
				},
				JsChange::ReplaceClosingParen {
					span: Span::new(inner.end, span.end),
				}
			],
			Self::Assignment {
				name,
				rhsspan,
				op,
				entirespan,
			} => smallvec![
				JsChange::AssignmentLeft {
					name,
					op,
					span: Span::new(entirespan.start, rhsspan.start)
				},
				JsChange::ReplaceClosingParen {
					span: Span::new(rhsspan.end, entirespan.end)
				}
			],
			// maps to insert
			Self::ShorthandObj { name, span } => smallvec![JsChange::ShorthandObj {
				ident: name,
				span: Span::new(span.end, span.end)
			}],
			// maps to insert
			Self::SourceTag { span } => smallvec![JsChange::SourceTag { span }],
			Self::Replace { text, span } => smallvec![JsChange::Replace { span, text }],
			Self::Delete { span } => smallvec![JsChange::Delete { span }],
			Self::GlobalFn { span } => smallvec![
				JsChange::GlobalFnLeft { span: Span::new(span.start, span.start) },
				JsChange::ClosingParen { span: Span::new(span.end, span.end), semi: false } // Re-use ClosingParen for the trailing ')'
			],
			Self::WindowMemberFn { span } => smallvec![
				JsChange::WindowMemberFnLeft { span: Span::new(span.start, span.start) },
				// This assumes the original span is for "obj.func" and we replace "obj.func" with "(0, window.func)".
				// The "func" part is implicitly handled by the original source code remaining after "obj." is skipped by WindowMemberFnLeft's span.
				// Or, if WindowMemberFnLeft replaces the entire "obj.func", then we need to re-insert "func" somehow or adjust spans.
				// For now, let's assume WindowMemberFnLeft handles "(0, window." and the original "func" remains, then we add ")".
				// A better approach for WindowMemberFn might be to replace the object part and keep the property.
				// Let's refine this. We want to change `obj.timer()` to `(0, window.timer)()`.
				// So, `WindowMemberFnLeft` should replace `obj.` with `(0, window.`
				// This means the span for `WindowMemberFnLeft` should be just `obj.` part.
				// However, the `span` passed to `WindowMemberFn` in `visitor.rs` is `member_expr.span()`.
				// This is the span of the entire `obj.timer`.
				//
				// Let's simplify:
				// Rewrite `ident` to `(0, ident)` using GlobalFnLeft and ClosingParen.
				// Rewrite `memberexpr` to `(0, window.memberexpr_property)`
				// This requires replacing `memberexpr.object` with `(0, window`.
				// This is getting complicated.
				//
				// Option 1: GlobalFn inserts `(0,` at start, `)` at end. Visitor provides span of `setTimeout`.
				//   `setTimeout` -> `(0, setTimeout)`
				// Option 2: WindowMemberFn inserts `(0, window.` at start of member_expr, `)` at end. Visitor provides span of `obj.setTimeout`.
				//   `obj.setTimeout` -> `(0, window.obj.setTimeout)` - THIS IS WRONG. We want `(0, window.setTimeout)`.
				//
				// The `WindowMemberFn` needs to replace `member_expr.object()` with `window` and then wrap the whole thing.
				// Or, more simply, replace the *entire* `member_expr` with `(0, window.property_name)`.
				// This means `WindowMemberFn` would take the `span` of the whole `member_expr` and the `property_name`.
				//
				// Let's reconsider the `Rewrite` variants and what `visitor.rs` provides.
				// `Rewrite::GlobalFn { span }` where span is for `setTimeout`. Output: `(0, setTimeout)`
				//   - JsChange::GlobalFnPrefix { span: span.start() } -> inserts "(0, "
				//   - JsChange::GlobalFnSuffix { span: span.end() }   -> inserts ")"
				//
				// `Rewrite::WindowMemberFn { span }` where span is for `obj.setTimeout`.
				// This needs to become `(0, window.setTimeout)`.
				// The `span` is for the whole `obj.setTimeout`.
				// We need to extract the property name (`setTimeout`) from the `MemberExpression` in `visitor.rs`
				// and pass it to `Rewrite::WindowMemberFn`.
				//
				// Let's assume `Rewrite::WindowMemberFn { span, property_name: Atom<'data> }`
				// Then it would be a single `JsChange::Replace { span, text: format!("(0, window.{})", property_name) }`
				// This is much simpler.
				//
				// For now, I'll stick to the simpler interpretation of GlobalFn and make WindowMemberFn similar,
				// assuming it will replace the whole expression. This means I need to adjust visitor.rs later.
				// OR, WindowMemberFn replaces just the object part.
				//
				// Let's go with:
				// GlobalFn { span } -> `(0, ` before `span`, `)` after `span`.
				// WindowMemberFn { span } -> `(0, window.` before `span`, `)` after `span`. THIS IS STILL WRONG for `obj.foo`.
				// It should be `(0, window.foo` not `(0, window.obj.foo`.
				//
				// Correct approach for WindowMemberFn:
				// It needs to replace `member_expr.object()` with `(0, window`, and keep `.` and `property`.
				// Span of `member_expr.object()`: from `member_expr.span().start` to `member_expr.object().span().end`.
				// Span of `.` : from `member_expr.object().span().end` to `member_expr.property().span().start`.
				// Span of `property`: `member_expr.property().span()`.
				//
				// This is too complex for a single `Rewrite` variant if we want to keep JsChange simple.
				//
				// Alternative for `obj.timerFunc()` -> `(0, window.timerFunc)()`
				// We need `Rewrite::ReplaceMemberWithWindowIndirect { span: member_expr.span(), property_span: member_expr.property().span() }`
				// The `JsChange` would then use `source[property_span]` to get "timerFunc"
				// and replace `span` with `(0, window.${source[property_span]})`
				// This seems like the most robust way.
				//
				// Let's define the new `Rewrite` variants as I initially planned in the previous step,
				// and then create corresponding `JsChange` that perform the simple prefix/suffix.
				// This means `WindowMemberFn` will incorrectly transform `obj.setTimeout` to `(0, window.obj.setTimeout)`.
				// I will note this down and correct it in the next iteration by passing the property name or adjusting spans.
				// For now, focus on getting the enum variants and basic JsChange plumbing.

				// GlobalFn { span } for `setTimeout` -> `(0, setTimeout)`
				//   JsChange::Insert { loc: span.start, text: "(0, " }
				//   JsChange::Insert { loc: span.end, text: ")" }
				// WindowMemberFn { span } for `obj.setTimeout` -> `(0, window.setTimeout)` (requires property extraction)
				// Let's assume for now `WindowMemberFn` replaces the entire expression.
				// This requires the `visitor.rs` to pass the property name.
				//
				// New plan:
				// Rewrite::GlobalFn { span } -> for identifiers like `setTimeout`
				// Rewrite::WindowMemberFn { span, property_name: Atom<'data> } -> for member expressions like `obj.setTimeout`
				//
				// I need to go back and change `visitor.rs` if I change the structure of `Rewrite` enum variants here.
				// Given the current step is to modify `changes.rs`, I will proceed with the simpler interpretation first
				// and refine if necessary. The `visitor.rs` currently passes `member_expr.span()` for `WindowMemberFn`.
				//
				// If `WindowMemberFn { span }` receives the span of `obj.setTimeout`:
				// And we want `(0, window.setTimeout)`
				// This means `JsChange::Replace { span, text: "(0, window.NAME)" }`.
				// This means `JsChange` needs the property name.
				// So `Rewrite::WindowMemberFn` must carry the property name.

				// Sticking to the previous step's plan:
				// `Rewrite::GlobalFn { span: ident_ref.span }`
				// `Rewrite::WindowMemberFn { span: member_expr.span() }`
				//
				// If `WindowMemberFn` gets `member_expr.span()`, to change `foo.bar()` to `(0, window.bar)()`,
				// the `JsChange` needs to know "bar".
				//
				// Let's define `Rewrite::ReplaceFullExpressionWithWindowIndirect { expression_span: Span, property_atom: Atom<'data> }`
				// And `Rewrite::WrapIdentifierWithIndirect { ident_span: Span }`
				//
				// Okay, looking at the existing `Rewrite` and `JsChange` structure:
				// `Rewrite` variants are decomposed into one or more `JsChange` primitive operations (Insert, Replace etc).
				// `JsChange` then has a `to_inner` that generates the actual text.
				//
				// For `setTimeout` -> `(0, setTimeout)`:
				// `Rewrite::GlobalFn { span }`
				//   -> `JsChange::InsertPrefix { span: span.start(), text: "(0, " }`
				//   -> `JsChange::InsertSuffix { span: span.end(), text: ")" }`
				//
				// For `obj.setTimeout` -> `(0, window.setTimeout)`:
				// `Rewrite::WindowMemberFn { span, property_name: Atom<'data> }` (visitor needs to provide property_name)
				//   -> `JsChange::Replace { span, text: format!("(0, window.{}", property_name) }`
				// This is the cleanest. I'll need to update `visitor.rs` later to pass `property_name`.
				// For this step, I will assume `WindowMemberFn` has `property_name`.
				// I will add it to the enum variant here.

				// Back to the original plan for this step, add new variants to Rewrite and JsChange
				// and their mappings, assuming the simplest interpretation for now, and refine.
				// The `ClosingParen` reuse is good for `GlobalFn`.

				// For `WindowMemberFn { span }` (where span is `obj.setTimeout`):
				// To make it `(0, window.setTimeout)`, we need `JsChange::Replace { span, text: ... }`
				// This requires `property_name` to be part of `WindowMemberFn`.
				// I will add `property_name: Atom<'data>` to `WindowMemberFn` variant.
				// This is a change from my previous thoughts for *this specific tool call*, but it's necessary for correctness.

				// So, the `Rewrite` enum will be:
				// GlobalFn { span }
				// WindowMemberFn { span, property_name: Atom<'data> }
				//
				// Then in `into_inner`:
				// GlobalFn -> JsChange::GlobalFnPrefix, JsChange::GlobalFnSuffix (or reuse existing ones if possible)
				// WindowMemberFn -> JsChange::Replace (this is simpler)

				// Let's use existing JsChange types if possible.
				// For GlobalFn { span } for `setTimeout` -> `(0, setTimeout)`
				//   JsChange::Insert { loc: span.start, text: "(0, " }
				//   JsChange::Insert { loc: span.end, text: ")" }
				// This requires two new JsChange variants or making Insert take SmallVec<Change>
				// The current JsChange::Insert takes a single `str`.
				// No, `JsChangeInner::Insert` takes `Changes<'a>`, which is `SmallVec<[Change<'a>; 8]>`.
				// So, `JsChange::Insert { span, text: Changes }` could work.
				// But `JsChange` itself doesn't store `Changes`. It stores specific types.
				//
				// Let's define new `JsChange` variants:
				// `JsChange::PrependGlobalIndirect { span }` -> inserts `(0, ` at `span.start`
				// `JsChange::AppendIndirectClose { span }` -> inserts `)` at `span.end`
				//
				// For `WindowMemberFn { span, property_name }` for `obj.setTimeout` -> `(0, window.setTimeout)`
				//   `JsChange::ReplaceWithWindowIndirect { span, property_name }` -> replaces `span` with text.
				// This seems like a good plan.

				JsChange::PrependGlobalIndirect { span: Span::new(span.start, span.start) },
				JsChange::AppendIndirectClose { span: Span::new(span.end, span.end) }
			],
			Self::WindowMemberFn { span, property_name } => {
				smallvec![JsChange::ReplaceFullWithWindowIndirect { span, property_name }]
			}
		}
	}
}

macro_rules! changes {
	[$($change:expr),+] => {
		smallvec![$(Change::from($change)),+]
    };
}

#[derive(Debug, PartialEq, Eq)]
enum JsChange<'data> {
	/// insert `${cfg.wrapfn}(`
	WrapFnLeft { span: Span, extra: bool },
	/// insert `,strictchecker)`
	WrapFnRight { span: Span, extra: bool },
	/// insert `${cfg.setrealmfn}({}).`
	SetRealmFn { span: Span },
	/// insert `${cfg.wrapthis}(`
	WrapThisFn { span: Span },
	/// insert `$scramerr(ident);`
	ScramErrFn { span: Span, ident: Atom<'data> },
	/// insert `$scramitize(`
	ScramitizeFn { span: Span },
	/// insert `eval(${cfg.rewritefn}(`
	EvalRewriteFn { span: Span },
	/// insert `: ${cfg.wrapfn}(ident)`
	ShorthandObj { span: Span, ident: Atom<'data> },
	/// insert scramtag
	SourceTag { span: Span },

	/// replace span with `${cfg.importfn}`
	ImportFn { span: Span },
	/// replace span with `${cfg.metafn}("${cfg.base}")`
	MetaFn { span: Span },
	/// replace span with `((t)=>$scramjet$tryset(${name},"${op}",t)||(${name}${op}t))(`
	AssignmentLeft {
		span: Span,
		name: Atom<'data>,
		op: AssignmentOperator,
	},

	/// replace span with `)`
	ReplaceClosingParen { span: Span },
	/// insert `)`
	ClosingParen { span: Span, semi: bool },

	PrependGlobalIndirect { span: Span },
	AppendIndirectClose { span: Span }, // For GlobalFn (paired with PrependGlobalIndirect)

	ReplaceFullWithWindowIndirect { span: Span, property_name: Atom<'data> }, // For WindowMemberFn

	/// replace span with text
	Replace { span: Span, text: String<'data> },
	/// replace span with ""
	Delete { span: Span },
}

impl JsChange<'_> {
	fn get_span(&self) -> &Span {
		match self {
			Self::WrapFnLeft { span, .. }
			| Self::WrapFnRight { span, .. }
			| Self::SetRealmFn { span }
			| Self::WrapThisFn { span }
			| Self::ScramErrFn { span, .. }
			| Self::ScramitizeFn { span }
			| Self::EvalRewriteFn { span }
			| Self::ShorthandObj { span, .. }
			| Self::SourceTag { span, .. }
			| Self::ImportFn { span }
			| Self::MetaFn { span }
			| Self::AssignmentLeft { span, .. }
			| Self::ReplaceClosingParen { span }
			| Self::ClosingParen { span, .. }
			| Self::PrependGlobalIndirect { span }
	| Self::AppendIndirectClose { span }
			| Self::ReplaceFullWithWindowIndirect { span, .. }
			| Self::Replace { span, .. }
			| Self::Delete { span } => span,
		}
	}

	fn to_inner<'alloc, 'change, E>(
		&'change self,
		cfg: &'change Config<'alloc, E>,
		offset: u32,
	) -> JsChangeInner<'change>
	where
		E: Fn(&str, &'alloc Allocator) -> String<'alloc>,
	{
		match self {
			Self::WrapFnLeft { span, extra } => {
				if *extra {
					JsChangeInner::Insert {
						loc: span.start,
						str: changes!["(", cfg.wrapfn, "("],
					}
				} else {
					JsChangeInner::Insert {
						loc: span.start,
						str: changes![cfg.wrapfn, "("],
					}
				}
			}
			Self::WrapFnRight { span, extra } => {
				if *extra {
					JsChangeInner::Insert {
						loc: span.start,
						str: changes![",", STRICTCHECKER, "))"],
					}
				} else {
					JsChangeInner::Insert {
						loc: span.start,
						str: changes![",", STRICTCHECKER, ")"],
					}
				}
			}
			Self::SetRealmFn { span } => JsChangeInner::Insert {
				loc: span.start,
				str: changes![cfg.setrealmfn, "({})."],
			},
			Self::WrapThisFn { span } => JsChangeInner::Insert {
				loc: span.start,
				str: changes![cfg.wrapthisfn, "("],
			},
			Self::ScramErrFn { span, ident } => JsChangeInner::Insert {
				loc: span.start,
				str: changes!["$scramerr(", ident, ");"],
			},
			Self::ScramitizeFn { span } => JsChangeInner::Insert {
				loc: span.start,
				str: changes![" $scramitize("],
			},
			Self::EvalRewriteFn { .. } => JsChangeInner::Replace {
				str: changes!["eval(", cfg.rewritefn, "("],
			},
			Self::ShorthandObj { span, ident } => JsChangeInner::Insert {
				loc: span.start,
				str: changes![":", cfg.wrapfn, "(", ident, ")"],
			},
			Self::SourceTag { span } => JsChangeInner::Insert {
				loc: span.start,
				str: changes!["/*scramtag ", span.start + offset, " ", cfg.sourcetag, "*/"],
			},
			Self::ImportFn { .. } => JsChangeInner::Replace {
				str: changes![cfg.importfn, "(\"", cfg.base, "\","],
			},
			Self::MetaFn { .. } => JsChangeInner::Replace {
				str: changes![cfg.metafn, "(\"", cfg.base, "\")"],
			},
			Self::AssignmentLeft { name, op, .. } => JsChangeInner::Replace {
				str: changes![
					"((t)=>$scramjet$tryset(",
					name,
					",\"",
					op,
					"\",t)||(",
					name,
					op,
					"t))("
				],
			},
			Self::ReplaceClosingParen { .. } => JsChangeInner::Replace { str: changes![")"] },
			Self::ClosingParen { span, semi } => JsChangeInner::Insert {
				loc: span.start,
				str: if *semi { changes![");"] } else { changes![")"] },
			},
			Self::PrependGlobalIndirect { span } => JsChangeInner::Insert {
				loc: span.start,
				str: changes!["(0, "],
			},
			Self::AppendIndirectClose { span } => JsChangeInner::Insert {
				loc: span.start,
				str: changes![")"],
			},
			Self::ReplaceFullWithWindowIndirect { span: _span, property_name } => JsChangeInner::Replace {
				// _span is not used directly here because JsChangeInner::Replace replaces the span associated with the JsChange itself.
				str: changes!["(0, window.", property_name, ")"],
			},
			Self::Replace { text, .. } => JsChangeInner::Replace {
				str: changes![text],
			},
			Self::Delete { .. } => JsChangeInner::Replace { str: changes![""] },
		}
	}
}

impl PartialOrd for JsChange<'_> {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for JsChange<'_> {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		match self.get_span().start.cmp(&other.get_span().start) {
			Ordering::Equal => match (self, other) {
				(Self::ScramErrFn { .. }, _) => Ordering::Less,
				(_, Self::ScramErrFn { .. }) => Ordering::Greater,
				_ => Ordering::Equal,
			},
			x => x,
		}
	}
}

enum Change<'a> {
	Str(&'a str),
	U32(u32),
}

impl<'a> From<&'a str> for Change<'a> {
	fn from(value: &'a str) -> Self {
		Self::Str(value)
	}
}

impl<'a> From<&'a String<'_>> for Change<'a> {
	fn from(value: &'a String<'_>) -> Self {
		Self::Str(value.as_str())
	}
}

impl<'a> From<&'a Atom<'_>> for Change<'a> {
	fn from(value: &'a Atom<'_>) -> Self {
		Self::Str(value.as_str())
	}
}

impl<'a> From<&'a AssignmentOperator> for Change<'a> {
	fn from(value: &'a AssignmentOperator) -> Self {
		Self::Str(value.as_str())
	}
}

impl From<u32> for Change<'static> {
	fn from(value: u32) -> Self {
		Self::U32(value)
	}
}

type Changes<'a> = SmallVec<[Change<'a>; 8]>;

enum JsChangeInner<'a> {
	Insert { loc: u32, str: Changes<'a> },
	Replace { str: Changes<'a> },
}

pub(crate) struct JsChangeResult<'alloc> {
	pub js: Vec<'alloc, u8>,
	pub sourcemap: Vec<'alloc, u8>,
}

pub(crate) struct JsChanges<'alloc: 'data, 'data> {
	alloc: &'alloc Allocator,
	inner: ChangeSet<'alloc, JsChange<'data>>,
}

impl<'alloc: 'data, 'data> JsChanges<'alloc, 'data> {
	pub fn new(alloc: &'alloc Allocator, capacity: usize) -> Self {
		Self {
			inner: ChangeSet::new(alloc, capacity),
			alloc,
		}
	}

	pub fn add(&mut self, rewrite: Rewrite<'data>) {
		for change in rewrite.into_inner() {
			self.inner.add(change);
		}
	}

	pub fn perform<E>(
		&mut self,
		js: &str,
		cfg: &Config<'alloc, E>,
	) -> Result<JsChangeResult<'alloc>, RewriterError>
	where
		E: Fn(&str, &'alloc Allocator) -> String<'alloc>,
	{
		let mut cursor = 0;
		let mut offset = 0i32;
		let mut buffer = Vec::with_capacity_in(js.len() * 2, self.alloc);

		macro_rules! tryget {
			($start:ident..$end:ident) => {
				js.get($start as usize..$end as usize)
					.ok_or_else(|| RewriterError::Oob($start, $end))?
			};
		}
		macro_rules! eval {
			($change:expr) => {
				match $change {
					Change::Str(x) => {
						buffer.extend_from_slice(x.as_bytes());
						x.len()
					}
					Change::U32(x) => {
						let x = format_compact_str!("{}", x);
						buffer.extend_from_slice(x.as_bytes());
						x.len()
					}
				}
			};
		}

		// insert has a 9 byte size, replace has a 13 byte minimum and usually it's like 5 bytes
		// for the old str added on so use 16 as a really rough estimate
		let mut map = Vec::with_capacity_in(self.inner.len() * 16, self.alloc);
		map.extend_from_slice(&(self.inner.len() as u32).to_le_bytes());

		self.inner.sort();

		for change in self.inner.iter() {
			let span = change.get_span();
			let start = span.start;
			let end = span.end;

			buffer.extend_from_slice(tryget!(cursor..start).as_bytes());

			match change.to_inner(cfg, cursor) {
				JsChangeInner::Insert { loc, str } => {
					let mut len = 0u32;
					buffer.extend_from_slice(tryget!(start..loc).as_bytes());
					for str in &str {
						len += eval!(str) as u32;
					}
					buffer.extend_from_slice(tryget!(loc..end).as_bytes());

					// INSERT op
					map.push(0);
					// pos
					map.extend_from_slice(&loc.wrapping_add_signed(offset).to_le_bytes());
					// size
					map.extend_from_slice(&len.to_le_bytes());

					offset = offset.wrapping_add_unsigned(len);
				}
				JsChangeInner::Replace { str } => {
					let mut len = 0u32;
					for str in &str {
						len += eval!(str) as u32;
					}

					// REPLACE op
					map.push(1);
					// len
					map.extend_from_slice(&(span.end - span.start).to_le_bytes());
					// start
					map.extend_from_slice(&(span.start.wrapping_add_signed(offset)).to_le_bytes());
					// end
					map.extend_from_slice(
						&((span.start + len).wrapping_add_signed(offset)).to_le_bytes(),
					);
					// oldstr
					map.extend_from_slice(tryget!(start..end).as_bytes());

					let len = i32::try_from(len).map_err(|_| RewriterError::AddedTooLarge)?;
					let diff = len.wrapping_sub_unsigned(span.end - span.start);
					offset = offset.wrapping_add(diff);
				}
			}

			cursor = end;
		}

		let js_len = js.len() as u32;
		buffer.extend_from_slice(tryget!(cursor..js_len).as_bytes());

		Ok(JsChangeResult {
			js: buffer,
			sourcemap: map,
		})
	}
}
