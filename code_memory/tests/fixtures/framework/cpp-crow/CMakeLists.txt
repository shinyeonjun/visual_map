cmake_minimum_required(VERSION 3.20)
project(cpp_crow_fixture LANGUAGES CXX)
add_executable(cpp_crow_fixture main.cpp handlers.cpp)
target_include_directories(cpp_crow_fixture PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
