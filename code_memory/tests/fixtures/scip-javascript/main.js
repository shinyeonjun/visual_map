const { User, box } = require("./types");

function add(left, right) {
  return left + right;
}

function run() {
  const user = box(new User("user-1"));
  return add(1, 2) + user.id.length;
}

const normalize = (value) => value;

module.exports = { add, normalize, run };
