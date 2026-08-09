import { localValue } from "./local";
import { readFile } from "node:fs";
import { missingValue } from "./missing";
export { localValue } from "./local";

const dynamicPath = "./local";
void import(dynamicPath);
// import { fake } from "./commented";
void [localValue, readFile, missingValue];
