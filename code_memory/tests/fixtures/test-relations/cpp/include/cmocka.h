#pragma once

struct CMUnitTest {
    void (*function)(void **state);
};

#define cmocka_unit_test(function_name) { function_name }
