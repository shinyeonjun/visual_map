#pragma once

int declared_value(int value);

class Contract {
public:
    virtual int run() const = 0;
};
