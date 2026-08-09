class BaseService {
  execute(value) {
    return value;
  }
}

class Service extends BaseService {
  execute(value) {
    const transient = new BaseService();
    return transient.execute(value);
  }
}

module.exports = { BaseService, Service };
