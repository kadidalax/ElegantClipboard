import { mergeDeep } from "../../core/merge";
import { core } from "./core";
import { extended } from "./extended";

export const zhCN = mergeDeep(
  core as Record<string, unknown>,
  extended as Record<string, unknown>,
);
