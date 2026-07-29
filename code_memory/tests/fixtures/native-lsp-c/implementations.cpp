#include "declarations.hpp"

int declared_value(int value) {
    return value + 1;
}

class Implementation final : public Contract {
public:
    int run() const override { return declared_value(1); }
};
