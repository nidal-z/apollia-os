/**
 * The model picker must always be able to display the configured model.
 *
 * Reproduces the reported defect: the select showed no value even with a model
 * loaded, while the status row beside it was right. The cause is a bound value
 * that matches no option, which Svelte resolves by selecting nothing.
 */
import { describe, it, expect } from "vitest";
import {
  buildModelOptions,
  selectedModelValue,
  modelValue,
  fileName,
} from "./modelOptions";
import type { SttModelInfo } from "$lib/types";

function model(name: string, overrides: Partial<SttModelInfo> = {}): SttModelInfo {
  return {
    name,
    path: `/Users/someone/.apollia/models/${name}`,
    size_mb: 1031,
    language: null,
    ...overrides,
  } as SttModelInfo;
}

describe("selectedModelValue", () => {
  it("normalises an absolute path to the option form", () => {
    // GIVEN the model path the onboarding persists, which is absolute
    const configured = "/Users/someone/.apollia/models/ggml-model-q5_0.bin";
    // WHEN the selected option value is derived
    const value = selectedModelValue(configured);
    // THEN it matches the value the options carry
    expect(value).toBe("~/.apollia/models/ggml-model-q5_0.bin");
  });

  it("is empty only when no model is configured", () => {
    expect(selectedModelValue("")).toBe("");
  });
});

describe("buildModelOptions", () => {
  it("selects the configured model when the scan returned it", () => {
    // GIVEN a scanned model and that same model configured by absolute path
    const scanned = [model("ggml-model-q5_0.bin")];
    const configured = "/Users/someone/.apollia/models/ggml-model-q5_0.bin";
    // WHEN the options are built
    const options = buildModelOptions(scanned, configured);
    // THEN one of them carries the selected value, so the field shows it
    expect(options.map((o) => o.value)).toContain(selectedModelValue(configured));
  });

  it("still offers a configured model the scan did not return", () => {
    // GIVEN a model imported as .gguf, which the directory scan used to skip
    const scanned = [model("ggml-model-q5_0.bin")];
    const configured = "~/.apollia/models/whisper-large-v3.gguf";
    // WHEN the options are built
    const options = buildModelOptions(scanned, configured);
    // THEN it is listed first, so the select is never handed an unmatched value
    expect(options[0]).toEqual({
      value: "~/.apollia/models/whisper-large-v3.gguf",
      label: "whisper-large-v3.gguf",
    });
    expect(options.map((o) => o.value)).toContain(selectedModelValue(configured));
  });

  it("still offers a configured model kept outside the models directory", () => {
    // GIVEN a model configured from another directory entirely
    const configured = "/Volumes/ssd/whisper/ggml-large-v3.bin";
    // WHEN the options are built against an empty scan
    const options = buildModelOptions([], configured);
    // THEN the field can still display it
    expect(options).toHaveLength(1);
    expect(options.map((o) => o.value)).toContain(selectedModelValue(configured));
  });

  it("never duplicates a model that the scan already returned", () => {
    // GIVEN a configured model present in the scan
    const scanned = [model("a.bin"), model("b.bin")];
    // WHEN the options are built
    const options = buildModelOptions(scanned, "~/.apollia/models/b.bin");
    // THEN the list is unchanged in length
    expect(options).toHaveLength(2);
  });

  it("returns nothing when no model is scanned and none configured", () => {
    // GIVEN a fresh install
    // WHEN the options are built
    // THEN the caller falls back to its "no models" message
    expect(buildModelOptions([], "")).toEqual([]);
  });

  it("labels scanned models with size and detected language", () => {
    const options = buildModelOptions(
      [model("whisper-fr.bin", { size_mb: 921.5, language: "fr" })],
      "",
    );
    expect(options[0].label).toBe("whisper-fr.bin (922 Mo · fr)");
  });
});

describe("fileName", () => {
  it("handles both separators", () => {
    expect(fileName("/a/b/c.bin")).toBe("c.bin");
    expect(fileName("C:\\models\\c.bin")).toBe("c.bin");
    expect(modelValue("c.bin")).toBe("~/.apollia/models/c.bin");
  });
});
