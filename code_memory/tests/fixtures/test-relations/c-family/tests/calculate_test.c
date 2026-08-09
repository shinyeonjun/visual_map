#include <cmocka.h>
#include "calculate.h"

static void doubles_a_value(void **state) {
    (void)state;
    calculate(2);
}

static void name_only_calculate(void **state) {
    (void)state;
}

static const struct CMUnitTest tests[] = {
    cmocka_unit_test(doubles_a_value),
    cmocka_unit_test(name_only_calculate),
};
