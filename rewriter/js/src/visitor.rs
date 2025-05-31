use oxc::{
	allocator::{Allocator, String},
	ast::ast::{
		AssignmentExpression, AssignmentTarget, CallExpression, DebuggerStatement,
		ExportAllDeclaration, ExportNamedDeclaration, Expression, FunctionBody,
		IdentifierReference, ImportDeclaration, ImportExpression, MemberExpression, MetaProperty,
		NewExpression, ObjectExpression, ObjectPropertyKind, ReturnStatement, ThisExpression,
		UnaryExpression, UnaryOperator, UpdateExpression,
	},
	ast_visit::{walk, Visit},
	span::{Atom, GetSpan, Span},
};

use crate::{
	cfg::Config,
	changes::{JsChanges, Rewrite},
};

// js MUST not be able to get a reference to any of these because sbx
//
// maybe move this out of this lib?
const UNSAFE_GLOBALS: &[&str] = &[
	"window",
	"self",
	"globalThis",
	"this",
	"parent",
	"top",
	"location",
	"document",
	"eval",
	"frames",
];

pub struct Visitor<'alloc, 'data, E>
where
	E: Fn(&str, &'alloc Allocator) -> String<'alloc>,
{
	pub jschanges: JsChanges<'alloc, 'data>,
	pub config: Config<'alloc, E>,
	pub alloc: &'alloc Allocator,
}

impl<'alloc, 'data, E> Visitor<'alloc, 'data, E>
where
	E: Fn(&str, &'alloc Allocator) -> String<'alloc>,
{
	fn rewrite_url(&mut self, url: Atom<'data>) -> oxc::allocator::String<'alloc> {
		let mut urlencoded = (self.config.urlrewriter)(&url, self.alloc);
		urlencoded.insert_str(0, self.config.prefix);
		urlencoded
	}

	fn rewrite_ident(&mut self, name: &Atom, span: Span) {
		if UNSAFE_GLOBALS.contains(&name.as_str()) {
			self.jschanges.add(Rewrite::WrapFn {
				span,
				wrapped: true,
			});
		}
	}

	fn walk_member_expression(&mut self, it: &Expression) -> bool {
		match it {
			Expression::Identifier(s) => {
				self.rewrite_ident(&s.name, s.span);
				true
			}
			Expression::StaticMemberExpression(s) => self.walk_member_expression(&s.object),
			Expression::ComputedMemberExpression(s) => self.walk_member_expression(&s.object),
			_ => false,
		}
	}

	fn scramitize(&mut self, span: Span) {
		self.jschanges.add(Rewrite::Scramitize { span });
	}
}

impl<'alloc, 'data, E> Visit<'data> for Visitor<'alloc, 'data, E>
where
	E: Fn(&str, &'alloc Allocator) -> String<'alloc>,
{
	fn visit_identifier_reference(&mut self, it: &IdentifierReference) {
		// if self.config.capture_errors {
		// 	self.jschanges.insert(JsChange::GenericChange {
		// 		span: it.span,
		// 		text: format!(
		// 			"{}({}, typeof arguments != 'undefined' && arguments)",
		// 			self.config.wrapfn, it.name
		// 		),
		// 	});
		// } else {
		//
		if UNSAFE_GLOBALS.contains(&it.name.as_str()) {
			self.jschanges.add(Rewrite::WrapFn {
				span: it.span,
				wrapped: false,
			});
		}
		// }
	}

	fn visit_new_expression(&mut self, it: &NewExpression<'data>) {
		self.walk_member_expression(&it.callee);
		walk::walk_arguments(self, &it.arguments);
	}

	fn visit_member_expression(&mut self, it: &MemberExpression<'data>) {
		// TODO
		// you could break this with ["postMessage"] etc
		// however this code only exists because of recaptcha whatever
		// and it would slow down js execution a lot
		if let MemberExpression::StaticMemberExpression(s) = it {
			if s.property.name == "postMessage" {
				self.jschanges.add(Rewrite::SetRealmFn {
					span: s.property.span,
				});

				walk::walk_expression(self, &s.object);
				return; // unwise to walk the rest of the tree
			}

			if !self.config.strict_rewrites && !UNSAFE_GLOBALS.contains(&s.property.name.as_str()) {
				if let Expression::Identifier(_) | Expression::ThisExpression(_) = &s.object {
					// cull tree - this should be safe
					return;
				}
			}

			if self.config.scramitize
				&& !matches!(s.object, Expression::MetaProperty(_) | Expression::Super(_))
			{
				self.scramitize(s.object.span());
			}
		}

		walk::walk_member_expression(self, it);
	}
	fn visit_this_expression(&mut self, it: &ThisExpression) {
		self.jschanges.add(Rewrite::WrapThisFn { span: it.span });
	}

	fn visit_debugger_statement(&mut self, it: &DebuggerStatement) {
		// delete debugger statements entirely. some sites will spam debugger as an anti-debugging measure, and we don't want that!
		self.jschanges.add(Rewrite::Delete { span: it.span });
	}

	// we can't overwrite window.eval in the normal way because that would make everything an
	// indirect eval, which could break things. we handle that edge case here
	fn visit_call_expression(&mut self, it: &CallExpression<'data>) {
		const TIMER_FUNCTIONS: &[&str] = &["setTimeout", "setInterval", "clearTimeout", "clearInterval"];

		let mut handled = false;

		match &it.callee {
			Expression::Identifier(ident_ref) => {
				if ident_ref.name == "eval" && !it.optional {
					self.jschanges.add(Rewrite::Eval {
						span: it.span,
						inner: Span::new(ident_ref.span.end + 1, it.span.end),
					});
					walk::walk_arguments(self, &it.arguments);
					return; // Early return for eval
				} else if TIMER_FUNCTIONS.contains(&ident_ref.name.as_str()) {
					// Rewrite setTimeout(args) to (0, setTimeout)(args)
					self.jschanges.add(Rewrite::GlobalFn { span: ident_ref.span });
					handled = true;
				}
			}
			Expression::MemberExpression(member_expr) => {
				if let Expression::Identifier(ident_ref) = member_expr.object().without_parenthesized() {
					// Check if it's something like `window.setTimeout` or `self.setTimeout`
					if UNSAFE_GLOBALS.contains(&ident_ref.name.as_str()) {
						if let Some(prop_name) = member_expr.static_property_name() {
							if TIMER_FUNCTIONS.contains(&prop_name) {
								// Already explicitly on window/self/etc., so no rewrite needed for binding.
								// However, we might still want to wrap it if it's an unsafe global.
								// For now, assume it's fine.
								// self.rewrite_ident(&ident_ref.name, ident_ref.span); //
								// This would wrap `window` if it's in UNSAFE_GLOBALS
								// walk::walk_member_expression(self, it.callee.to_member_expression());
								// handled = true;
							}
						}
					} else {
						// It's obj.setTimeout, but obj is not window.
						// Rewrite obj.setTimeout(args) to (0, window.setTimeout)(args)
						if let Some(prop_name_atom) = member_expr.static_property_name() {
							if TIMER_FUNCTIONS.contains(&prop_name_atom.as_str()) {
								self.jschanges.add(Rewrite::WindowMemberFn {
									span: member_expr.span(),
									property_name: prop_name_atom.clone(), // Atom can be cloned
								});
								handled = true;
							}
						}
					}
				} else if let Some(prop_name_atom) = member_expr.static_property_name() {
					// Catches cases like `this.setTimeout` or `some_other_object.setTimeout`
					// For `this.setTimeout`, it should ideally be bound if `this` isn't window.
					// For `some_other_object.setTimeout`, we also want to rebind to `window`.
					if TIMER_FUNCTIONS.contains(&prop_name_atom.as_str()) {
						self.jschanges.add(Rewrite::WindowMemberFn {
							span: member_expr.span(),
							property_name: prop_name_atom.clone(), // Atom can be cloned
						});
						handled = true;
					}
				}
			}
			_ => {}
		}

		if self.config.scramitize && !handled { // Potentially avoid double scramitizing if handled
			self.scramitize(it.span);
		}

		// If handled, we've already decided what to do with the callee.
		// We still need to walk arguments.
		// If not handled, walk the whole call expression.
		if handled {
			walk::walk_arguments(self, &it.arguments);
			// Potentially walk other parts of CallExpression like type parameters if necessary
		} else {
			walk::walk_call_expression(self, it);
		}
	}

	fn visit_import_declaration(&mut self, it: &ImportDeclaration<'data>) {
		let text = self.rewrite_url(it.source.value);
		self.jschanges.add(Rewrite::Replace {
			span: it.source.span.shrink(1),
			text,
		});
		walk::walk_import_declaration(self, it);
	}
	fn visit_import_expression(&mut self, it: &ImportExpression<'data>) {
		self.jschanges.add(Rewrite::ImportFn {
			span: Span::new(it.span.start, it.span.start + 7),
		});
		walk::walk_import_expression(self, it);
	}

	fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'data>) {
		let text = self.rewrite_url(it.source.value);
		self.jschanges.add(Rewrite::Replace {
			span: it.source.span.shrink(1),
			text,
		});
	}
	fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'data>) {
		if let Some(source) = &it.source {
			let text = self.rewrite_url(source.value);
			self.jschanges.add(Rewrite::Replace {
				span: source.span.shrink(1),
				text,
			});
		}
		// do not walk further, we don't want to rewrite the identifiers
	}

	#[cfg(feature = "debug")]
	fn visit_try_statement(&mut self, it: &oxc::ast::ast::TryStatement<'data>) {
		// for debugging we need to know what the error was

		if self.config.capture_errors {
			if let Some(h) = &it.handler {
				if let Some(name) = &h.param {
					if let Some(ident) = name.pattern.get_identifier_name() {
						self.jschanges.add(Rewrite::ScramErr {
							span: Span::new(h.body.span.start + 1, h.body.span.start + 1),
							ident,
						});
					}
				}
			}
		}
		walk::walk_try_statement(self, it);
	}

	fn visit_object_expression(&mut self, it: &ObjectExpression<'data>) {
		for prop in &it.properties {
			if let ObjectPropertyKind::ObjectProperty(p) = prop {
				if let Expression::Identifier(s) = &p.value {
					if UNSAFE_GLOBALS.contains(&s.name.to_string().as_str()) && p.shorthand {
						self.jschanges.add(Rewrite::ShorthandObj {
							span: s.span,
							name: s.name,
						});
						return;
					}
				}
			}
		}

		walk::walk_object_expression(self, it);
	}

	fn visit_function_body(&mut self, it: &FunctionBody<'data>) {
		// tag function for use in sourcemaps
		if self.config.do_sourcemaps {
			self.jschanges.add(Rewrite::SourceTag {
				span: Span::new(it.span.start, it.span.start),
			});
		}
		walk::walk_function_body(self, it);
	}

	fn visit_return_statement(&mut self, it: &ReturnStatement<'data>) {
		// if let Some(arg) = &it.argument {
		// 	self.jschanges.insert(JsChange::GenericChange {
		// 		span: Span::new(it.span.start + 6, it.span.start + 6),
		// 		text: format!(" $scramdbg((()=>{{ try {{return arguments}} catch(_){{}} }})(),("),
		// 	});
		// 	self.jschanges.insert(JsChange::GenericChange {
		// 		span: Span::new(expression_span(arg).end, expression_span(arg).end),
		// 		text: format!("))"),
		// 	});
		// }
		walk::walk_return_statement(self, it);
	}

	fn visit_unary_expression(&mut self, it: &UnaryExpression<'data>) {
		if matches!(it.operator, UnaryOperator::Typeof) {
			// don't walk to identifier rewrites since it won't matter
			return;
		}
		walk::walk_unary_expression(self, it);
	}

	fn visit_update_expression(&mut self, _it: &UpdateExpression<'data>) {
		// then no, don't walk it, we don't care
	}

	fn visit_meta_property(&mut self, it: &MetaProperty<'data>) {
		if it.meta.name == "import" {
			self.jschanges.add(Rewrite::MetaFn { span: it.span });
		}
	}

	fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'data>) {
		match &it.left {
			AssignmentTarget::AssignmentTargetIdentifier(s) => {
				if ["location"].contains(&s.name.to_string().as_str()) {
					self.jschanges.add(Rewrite::Assignment {
						name: s.name,
						entirespan: it.span,
						rhsspan: it.right.span(),
						op: it.operator,
					});

					// avoid walking rest of tree, i would need to figure out nested rewrites
					// somehow
					return;
				}
			}
			AssignmentTarget::ArrayAssignmentTarget(_) => {
				// [location] = ["https://example.com"]
				// this is such a ridiculously specific edge case. just ignore it
				return;
			}
			_ => {
				// only walk the left side if it isn't an identifier, we can't replace the
				// identifier with a function obviously
				walk::walk_assignment_target(self, &it.left);
			}
		}
		walk::walk_expression(self, &it.right);
	}
}
