import { mergeDeep } from "../../core/merge";
import { core } from "./core";
import { extended } from "./extended";
import type { TranslationTree } from "../../types";

export const en = mergeDeep(
  core as Record<string, unknown>,
  extended as Record<string, unknown>,
) as TranslationTree;
