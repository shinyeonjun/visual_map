import express from "express";
import { health } from "./handlers";

const app = express();

export function authMiddleware() {
  return true;
}

app.use(authMiddleware);
app.get("/health", health);
app.get("/unknown", missingHandler);
