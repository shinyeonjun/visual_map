#include "types.h"

int add(int left, int right) {
    return left + right;
}

int main(void) {
    Box box = {{"user-1"}};
    return add(1, 2) + (int)box_id(&box)[0];
}
