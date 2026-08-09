package main

import (
    "example.com/importgroundtruth/local"
    "fmt"
    _ "example.com/missing/dependency"
)

// import "example.com/commented/fake"
func main() { fmt.Println(local.Value()) }
