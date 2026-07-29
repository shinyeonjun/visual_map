import { Box, User } from "./types";

export function add(a: number, b: number): number {
  return a + b;
}

export function run(): number {
  const box = new Box(new User("user-1"));
  return add(1, 2) + box.get().id.length;
}
