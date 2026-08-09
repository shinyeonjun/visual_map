const local = require("./local");
const fs = require("fs");
const missing = require("./missing");
const dynamicPath = "./local";
require(dynamicPath);
// require("./commented");
module.exports = { local, fs, missing };
