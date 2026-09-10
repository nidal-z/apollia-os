import { describe, expect, it } from "vitest";
import { modelRowState } from "./modelRowState";

describe("modelRowState", () => {
  it("reads a configured engine that has not transcribed yet as ready", () => {
    // GIVEN an armed engine whose model has not served a request yet, which
    // is what every reload produces
    const status = { enabled: true, model_loaded: false, model_name: "ggml-model-q5_0.bin" };

    // WHEN the row state is derived
    // THEN it is ready, not "not loaded": the model loads on the first request
    expect(modelRowState(status)).toBe("ready");
  });

  it("reads a served engine as loaded", () => {
    // GIVEN an engine that returned a transcription
    const status = { enabled: true, model_loaded: true, model_name: "ggml-model-q5_0.bin" };

    // WHEN the row state is derived
    // THEN the model is reported resident
    expect(modelRowState(status)).toBe("loaded");
  });

  it("keeps absent for a disabled engine, an unnamed model, or no status", () => {
    // GIVEN the three shapes that carry no model to name. This is the control:
    // a derivation that answered "ready" on an empty name would print a blank
    // model as ready.
    // WHEN the row state is derived
    // THEN all three are absent
    expect(modelRowState({ enabled: false, model_loaded: false, model_name: "x.bin" })).toBe("absent");
    expect(modelRowState({ enabled: true, model_loaded: false, model_name: "  " })).toBe("absent");
    expect(modelRowState(null)).toBe("absent");
  });
});
