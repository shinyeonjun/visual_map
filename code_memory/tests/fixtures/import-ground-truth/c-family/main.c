#include "include/local.h"
#include <stdio.h>
#define C_HEADER_ALIAS "include/local.h"
#include C_HEADER_ALIAS
// #include "commented.h"

int main(void) { return LOCAL_C_VALUE; }
