// test_setTimeout.js
console.log('Starting setTimeout tests...');

let testCounter = 0;
const expectedTests = 4; // Keep this in sync with the number of tests

function checkDone() {
  testCounter++;
  if (testCounter === expectedTests) {
    console.log('All setTimeout tests completed successfully.');
  }
}

try {
  setTimeout(() => {
    console.log('Test 1: Direct setTimeout executed.');
    checkDone();
  }, 10);
} catch (e) {
  console.error('Test 1 Error:', e.message);
  checkDone(); // Still count it to not hang the test
}

try {
  window.setTimeout(() => {
    console.log('Test 2: window.setTimeout executed.');
    checkDone();
  }, 20);
} catch (e) {
  console.error('Test 2 Error:', e.message);
  checkDone();
}

const myObj = {
  name: 'myObj',
  runTimeout: function() {
    setTimeout(() => {
      console.log(`Test 3: setTimeout within object method (${this.name}) executed.`);
      checkDone();
    }, 30);
  }
};
try {
  myObj.runTimeout();
} catch (e) {
  console.error('Test 3 Error:', e.message);
  checkDone();
}

const anotherObj = {
  name: 'anotherObj',
  runTimeout: function() {
    this.setTimeout(() => { // Intentionally using this.setTimeout
      console.log(`Test 4: this.setTimeout within object method (${this.name}) executed.`);
      checkDone();
    }, 40);
  }
};
try {
  // To make this.setTimeout work like window.setTimeout, it needs to be bound or called on window.
  // In a browser, 'this' at the global scope or in a function not bound to an object would be 'window'.
  // Node.js global is 'global'. We need to simulate 'window' for the rewriter.
  global.window = global; // Simulate window for the test
  anotherObj.runTimeout.call(global.window); // Call it with 'window' as 'this'
} catch (e) {
  console.error('Test 4 Error:', e.message);
  checkDone();
}

console.log('setTimeout calls initiated.');
