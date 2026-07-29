#pragma once

struct App {
  App& route(const char* path);
};

#define CROW_ROUTE(app, path) (app).route(path)
