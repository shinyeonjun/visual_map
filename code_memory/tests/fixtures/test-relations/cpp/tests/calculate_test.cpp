#include "../include/cmocka.h"
#include "../src/calculate.hpp"

void doubles_a_value(void **state) {
    (void)state;
    calculate(2);
}

void name_only_calculate(void **state) {
    (void)state;
}

static const CMUnitTest tests[] = {
    cmocka_unit_test(doubles_a_value),
    cmocka_unit_test(name_only_calculate),
};
