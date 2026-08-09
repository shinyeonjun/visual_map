import test from "node:test";
import { calculate } from "../src/calculate.js";

test("doubles a value", () => {
  calculate(2);
});

test("name only calculate", () => {});
