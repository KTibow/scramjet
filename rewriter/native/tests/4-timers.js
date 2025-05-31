// Test cases for setTimeout, setInterval, clearTimeout, clearInterval

// --- setTimeout ---
console.log("--- setTimeout Tests ---");

// obj.setTimeout(...)
const myObjSetTimeout = {
  st: setTimeout,
  name: "myObjSetTimeout",
  log: function(msg) { console.log(this.name + ": " + msg); }
};
myObjSetTimeout.st(() => myObjSetTimeout.log("obj.setTimeout direct"), 10);

const anotherObjSetTimeout = {
  stVal: setTimeout
};
anotherObjSetTimeout.stVal(() => console.log("anotherObjSetTimeout: obj.setTimeout via stVal"), 10);


// variableSetTimeout(...)
const myVarSetTimeout = setTimeout;
myVarSetTimeout(() => console.log("variableSetTimeout: direct call"), 10);

// thisSetTimeoutInMethod(...)
const anObjWithMethodTimeout = {
  name: "anObjWithMethodTimeout",
  st: setTimeout,
  log: function(msg) { console.log(this.name + ": " + msg); },
  method: function() {
    this.st(() => this.log("this.setTimeout in method"), 10);
  }
};
anObjWithMethodTimeout.method();

// windowSetTimeout(...) (should not be significantly changed)
window.setTimeout(() => console.log("window.setTimeout: direct call"), 10);
self.setTimeout(() => console.log("self.setTimeout: direct call"), 10);

// --- setInterval ---
console.log("--- setInterval Tests ---");

// obj.setInterval(...)
const myObjSetInterval = {
  si: setInterval,
  name: "myObjSetInterval",
  log: function(msg) { console.log(this.name + ": " + msg); }
};
const intervalIdObj = myObjSetInterval.si(() => myObjSetInterval.log("obj.setInterval direct"), 20);
clearInterval(intervalIdObj); // Clear to prevent spamming

// variableSetInterval(...)
const myVarSetInterval = setInterval;
const intervalIdVar = myVarSetInterval(() => console.log("variableSetInterval: direct call"), 20);
clearInterval(intervalIdVar); // Clear

// window.setInterval(...) (should not be significantly changed)
const intervalIdWindow = window.setInterval(() => console.log("window.setInterval: direct call"), 20);
clearInterval(intervalIdWindow); // Clear
const intervalIdSelf = self.setInterval(() => console.log("self.setInterval: direct call"), 20);
clearInterval(intervalIdSelf); // Clear


// --- clearTimeout ---
console.log("--- clearTimeout Tests ---");
const timeoutIdForClear = setTimeout(() => {}, 30);

// obj.clearTimeout(...)
const myObjClearTimeout = {
  ct: clearTimeout,
  name: "myObjClearTimeout"
};
myObjClearTimeout.ct(timeoutIdForClear);
console.log("myObjClearTimeout: Cleared timeoutIdForClear using obj.ct");

// variableClearTimeout(...)
const myVarClearTimeout = clearTimeout;
const anotherTimeoutIdForClearVar = setTimeout(() => {}, 30);
myVarClearTimeout(anotherTimeoutIdForClearVar);
console.log("variableClearTimeout: Cleared anotherTimeoutIdForClearVar using myVarClearTimeout");

// window.clearTimeout(...) (should not be significantly changed)
const yetAnotherTimeoutId = setTimeout(() => {}, 30);
window.clearTimeout(yetAnotherTimeoutId);
console.log("window.clearTimeout: Cleared yetAnotherTimeoutId using window.clearTimeout");
const selfTimeoutId = setTimeout(() => {}, 30);
self.clearTimeout(selfTimeoutId);
console.log("self.clearTimeout: Cleared selfTimeoutId using self.clearTimeout");


// --- clearInterval ---
console.log("--- clearInterval Tests ---");
const intervalIdForClear = setInterval(() => {}, 40);

// obj.clearInterval(...)
const myObjClearInterval = {
  ci: clearInterval,
  name: "myObjClearInterval"
};
myObjClearInterval.ci(intervalIdForClear);
console.log("myObjClearInterval: Cleared intervalIdForClear using obj.ci");

// variableClearInterval(...)
const myVarClearInterval = clearInterval;
const anotherIntervalIdForClearVar = setInterval(() => {}, 40);
myVarClearInterval(anotherIntervalIdForClearVar);
console.log("variableClearInterval: Cleared anotherIntervalIdForClearVar using myVarClearInterval");

// window.clearInterval(...) (should not be significantly changed)
const yetAnotherIntervalId = setInterval(() => {}, 40);
window.clearInterval(yetAnotherIntervalId);
console.log("window.clearInterval: Cleared yetAnotherIntervalId using window.clearInterval");

const selfIntervalId = setInterval(() => {}, 40);
self.clearInterval(selfIntervalId);
console.log("self.clearInterval: Cleared selfIntervalId using self.clearInterval");

// Test case for `this.st` where `st` might be shadowed or on prototype (simplified)
const complexThisObj = {
  st: function(cb, delay) { console.error("SHOULD NOT CALL THIS ST", cb, delay); }, // Shadow
  method: function() {
    // Assuming 'this.timerFunc' should resolve to global 'setTimeout' or 'window.setTimeout'
    // if 'timerFunc' is not a direct property of 'complexThisObj'
    // This specific rewrite logic is `(0, window.timerFunc)` which is global.
    // If `this.timerFunc` was intended to be `complexThisObj.timerFunc` and that was `setTimeout`,
    // the rewrite `(0, window.setTimeout)` would still be "correct" under the goal of global binding.

    // To test the scenario from the prompt: anObj.method calls this.st, st is setTimeout
    // This was covered by anObjWithMethodTimeout.
    // Let's consider a slightly different one:
    // What if 'st' is not on 'this' but inherited, and it's 'setTimeout'?
    // The rewrite always makes it (0, window.setTimeout), so 'this' context before rewrite doesn't matter for the call itself.
    console.log("complexThisObj.method called - this test is more for reasoning");
  }
};
complexThisObj.st = setTimeout; // Assign global setTimeout to the object's property
complexThisObj.method(); // Does nothing with timers, but complexThisObj.st is now setTimeout
complexThisObj.st(() => console.log("complexThisObj.st direct call after assignment"), 10);


// Ensure eval isn't broken
console.log("--- Eval test with setTimeout ---");
eval("setTimeout(() => console.log('eval setTimeout'), 50)");

// Ensure direct window calls are minimally affected for all timers
console.log("--- Direct window.* calls ---");
window.setTimeout(() => console.log("direct window.setTimeout"), 10);
const id_wst = window.setInterval(() => { console.log("direct window.setInterval"); window.clearInterval(id_wst); }, 10);
const id_wct = window.setTimeout(() => console.log("this should be cleared by window.clearTimeout"), 10);
window.clearTimeout(id_wct);
const id_wci = window.setInterval(() => console.log("this should be cleared by window.clearInterval"), 10);
window.clearInterval(id_wci);

console.log("--- End of Timer Tests ---");
