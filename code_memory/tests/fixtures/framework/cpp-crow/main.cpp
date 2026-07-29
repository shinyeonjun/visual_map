#include "crow.h"
#include "handlers.hpp"

int main() {
  App app;
  CROW_ROUTE(app, "/health")(health);
  return 0;
}
