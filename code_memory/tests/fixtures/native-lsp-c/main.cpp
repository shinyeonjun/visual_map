#include "types.h"

class Base {
public:
    virtual int base_value() const { return 1; }
};

class Runner {
public:
    virtual int run() const = 0;
};

template <typename T>
class BoxValue : public Base {
public:
    explicit BoxValue(T value) : value_(value) {}
    T get() const { return value_; }

private:
    T value_;
};

int multiply(int left, int right) {
    return left * right;
}

int main() {
    BoxValue<int> box(2);
    return multiply(box.get(), 3);
}
