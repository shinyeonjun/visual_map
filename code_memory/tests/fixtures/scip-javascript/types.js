class BaseEntity {
  constructor(id) {
    this.id = id;
  }
}

class User extends BaseEntity {}

/** @template T @param {T} value @returns {T} */
function box(value) {
  return value;
}

class Handler {
  execute = (value) => value;
}

module.exports = { Handler, User, box };
