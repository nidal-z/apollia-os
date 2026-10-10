import { describe, expect, test, vi } from "vitest";
import type { DictationModel, DictationModels } from "$lib/ipc/modelHub";
import type { SttConfigView, SttModelInfo } from "$lib/types";
import { asSetupModels, dictationDownload, modelInUse, useDictationModel } from "./dictationSetup";

function entry(over: Partial<DictationModel> = {}): DictationModel {
  return {
    id: "whisper-small",
    label: "Whisper small",
    publisher: "OpenAI",
    released: "2022-09-21",
    license: "MIT",
    repo: "ggerganov/whisper.cpp",
    file: "ggml-small-q5_1.bin",
    quantization: "q5_1",
    size_bytes: 190_000_000,
    sha256: "aa",
    card: "whisper/small",
    footprint_gb: 0.6,
    fits: true,
    ...over,
  };
}

function config(over: Partial<SttConfigView> = {}): SttConfigView {
  return {
    enabled: false,
    model_path: "",
    hotkey: "F9",
    clipboard_mode: "paste",
    clipboard_restore: true,
    silence_threshold_db: -40,
    max_recording_sec: 60,
    language: null,
    trigger_mode: "hold",
    input_device: null,
    ...over,
  };
}

const MODELS: DictationModels = { models: [entry()], recommended: "whisper-small" };
const INSTALLED: SttModelInfo[] = [
  { name: "ggml-small-q5_1.bin", path: "/models/ggml-small-q5_1.bin", size_mb: 181, language: null },
];

describe("the dictation model, read by the setup of the first launch", () => {
  test("the models of the registry are offered with the one for this machine", () => {
    // GIVEN the dictation models and the recommended one
    // WHEN they are read for the setup
    const offered = asSetupModels(MODELS);

    // THEN the same entries are offered, the recommended one named, with no reason to show
    expect(offered.models.map((m) => [m.id, m.repo, m.file, m.size_bytes])).toEqual([
      ["whisper-small", "ggerganov/whisper.cpp", "ggml-small-q5_1.bin", 190_000_000],
    ]);
    expect(offered.recommended).toBe("whisper-small");
    expect(offered.reason).toBeNull();
  });

  test("a model counts as in use only when dictation is on and its file is installed", () => {
    // GIVEN dictation on with an installed file, off with the same file, and
    // on with a file that is no longer on this machine
    const on = config({ enabled: true, model_path: "/models/ggml-small-q5_1.bin" });
    const off = config({ enabled: false, model_path: "/models/ggml-small-q5_1.bin" });
    const gone = config({ enabled: true, model_path: "/models/ggml-base.bin" });

    // WHEN the model in use is read in each case
    // THEN only the first names one
    expect(modelInUse(on, INSTALLED, MODELS.models).model).toBe("whisper-small");
    expect(modelInUse(off, INSTALLED, MODELS.models).model).toBeNull();
    expect(modelInUse(gone, INSTALLED, MODELS.models).model).toBeNull();
  });

  test("a dictation file is fetched as one file at the address of its repository", () => {
    // GIVEN the download the setup asks for
    // WHEN it is turned into the download of one file
    const download = dictationDownload({
      repo_id: "ggerganov/whisper.cpp",
      selector: { kind: "gguf", file: "ggml-small-q5_1.bin" },
    });

    // THEN the address, the file name and the repository are those of the entry
    expect(download).toEqual({
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
      filename: "ggml-small-q5_1.bin",
      repo_id: "ggerganov/whisper.cpp",
    });
  });

  test("putting a model in use names its file in the settings, switches dictation on and reloads", async () => {
    // GIVEN dictation off and the file of the model installed
    const save = vi.fn().mockResolvedValue(undefined);
    const reload = vi.fn().mockResolvedValue(undefined);
    const deps = {
      models: async () => MODELS,
      installed: async () => INSTALLED,
      config: async () => config({ language: "fr" }),
      save,
      reload,
    };

    // WHEN the model is put in use
    const used = await useDictationModel("whisper-small", deps);

    // THEN the settings keep what they held, name the file and switch dictation on
    expect(save).toHaveBeenCalledWith(
      config({ language: "fr", enabled: true, model_path: "/models/ggml-small-q5_1.bin" }),
    );
    expect(reload).toHaveBeenCalledTimes(1);
    expect(used).toEqual({ model: "whisper-small", installed: true, path: "/models/ggml-small-q5_1.bin" });
  });

  test("a model whose file is not on this machine is not put in use", async () => {
    // GIVEN a model of the registry whose file was not installed
    const save = vi.fn();
    const deps = {
      models: async () => MODELS,
      installed: async () => [],
      config: async () => config(),
      save,
      reload: vi.fn(),
    };

    // WHEN it is put in use
    const refused = useDictationModel("whisper-small", deps);

    // THEN it is refused, naming the file, and the settings are left alone
    await expect(refused).rejects.toThrow("ggml-small-q5_1.bin");
    expect(save).not.toHaveBeenCalled();
  });
});
