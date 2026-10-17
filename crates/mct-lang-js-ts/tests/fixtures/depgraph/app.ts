import { helper } from "./lib/util";
import "./theme.css";
import { cfg } from "./legacy.js";
import * as path from "node:path";
import React from "react";

export const legacy = require("./lib/old");

export async function load() {
  return import("./lib/lazy");
}

helper(cfg, path, React, legacy);
