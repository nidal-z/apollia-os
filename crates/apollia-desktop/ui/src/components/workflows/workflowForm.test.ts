import { describe, expect, it } from "vitest";
import type { WorkflowField, WorkflowInputKind } from "$lib/ipc/workflows";
import {
  addItem,
  controlOf,
  emptyValue,
  fieldError,
  formErrors,
  initialValues,
  parseRefusal,
  toggleItem,
  toInputs,
} from "./workflowForm";

function field(name: string, kind: WorkflowInputKind, more: Partial<WorkflowField> = {}): WorkflowField {
  return { name, kind, required: false, nullable: false, ...more };
}

describe("the control of a field, by type", () => {
  it("gives each kind of input the control that carries it", () => {
    // GIVEN one field of each kind the engine declares
    const kinds: [WorkflowField, string][] = [
      [field("a", "text"), "text"],
      [field("b", "long_text"), "textarea"],
      [field("c", "file"), "file"],
      [field("d", "date"), "date"],
      [field("e", "integer"), "number"],
      [field("f", "number"), "number"],
      [field("g", "boolean"), "toggle"],
      [field("h", "choice", { choices: ["x", "y"] }), "select"],
      [field("i", "list", { itemKind: "choice", choices: ["x", "y"] }), "checks"],
      [field("j", "list", { itemKind: "text" }), "items"],
      [field("k", "other"), "json"],
    ];

    // WHEN the control of each is asked
    const controls = kinds.map(([f]) => controlOf(f));

    // THEN each kind has its own control
    expect(controls).toEqual(kinds.map(([, control]) => control));
  });

  it("starts a switch off, a list empty and every other control blank", () => {
    // GIVEN a switch, two lists and a text
    // WHEN the value each starts with is asked
    // THEN the switch is off, the lists are empty, the text is blank
    expect(emptyValue(field("g", "boolean"))).toBe(false);
    expect(emptyValue(field("i", "list", { choices: ["x"] }))).toEqual([]);
    expect(emptyValue(field("j", "list"))).toEqual([]);
    expect(emptyValue(field("a", "text"))).toBe("");
  });
});

describe("the values a form opens with", () => {
  const fields = [
    field("delay", "integer", { required: true }),
    field("notify", "boolean"),
    field("tags", "list", { itemKind: "text" }),
    field("extra", "other"),
    field("note", "text"),
  ];

  it("opens empty when nothing is carried over", () => {
    // GIVEN a form opened by "Launch"
    // WHEN its values are built
    const values = initialValues(fields);

    // THEN every control holds its empty value
    expect(values).toEqual({ delay: "", notify: false, tags: [], extra: "", note: "" });
  });

  it("opens filled with the inputs of the run launched again", () => {
    // GIVEN the inputs of a past run, typed as the engine stored them
    const previous = { delay: 7, notify: true, tags: ["a", "b"], extra: { k: 1 } };

    // WHEN the form is opened to launch it again
    const values = initialValues(fields, previous);

    // THEN each control shows the input as text, a switch as its answer, and
    // an input the run did not hold stays empty
    expect(values).toEqual({ delay: "7", notify: true, tags: ["a", "b"], extra: '{"k":1}', note: "" });
  });
});

describe("the values refused before the launch", () => {
  it("refuses a required input left empty and lets an optional one through", () => {
    // GIVEN a required and an optional text, both empty
    // WHEN each is checked
    // THEN only the required one is refused
    expect(fieldError(field("a", "text", { required: true }), "  ")).toBe("required");
    expect(fieldError(field("a", "text"), "")).toBeNull();
    expect(fieldError(field("a", "list", { required: true }), [])).toBe("required");
  });

  it("never refuses a switch, which always holds an answer", () => {
    // GIVEN a required yes or no left off
    // WHEN it is checked
    // THEN it is accepted
    expect(fieldError(field("g", "boolean", { required: true }), false)).toBeNull();
  });

  it("refuses a value of another type than the field declares", () => {
    // GIVEN values that do not fit their field
    // WHEN each is checked
    // THEN each is refused for its own reason
    expect(fieldError(field("e", "integer"), "3.5")).toBe("not_an_integer");
    expect(fieldError(field("f", "number"), "three")).toBe("not_a_number");
    expect(fieldError(field("d", "date"), "04/10/2026")).toBe("malformed_date");
    expect(fieldError(field("h", "choice", { choices: ["x"] }), "z")).toBe("not_a_choice");
    expect(fieldError(field("k", "other"), "{not json")).toBe("not_json");
    expect(fieldError(field("j", "list", { itemKind: "integer" }), ["1", "two"])).toBe("not_an_integer");
    expect(fieldError(field("i", "list", { choices: ["x"] }), ["x", "q"])).toBe("not_a_choice");
  });

  it("accepts a value that fits its field", () => {
    // GIVEN values that fit their field
    // WHEN each is checked
    // THEN none is refused
    expect(fieldError(field("e", "integer"), " -12 ")).toBeNull();
    expect(fieldError(field("f", "number"), "3.5")).toBeNull();
    expect(fieldError(field("d", "date"), "2026-10-04")).toBeNull();
    expect(fieldError(field("h", "choice", { choices: ["x"] }), "x")).toBeNull();
    expect(fieldError(field("k", "other"), '{"a":[1]}')).toBeNull();
  });

  it("marks every field at fault at once", () => {
    // GIVEN a form with two fields at fault and one that fits
    const fields = [
      field("delay", "integer", { required: true }),
      field("file", "file", { required: true }),
      field("note", "text"),
    ];

    // WHEN the form is checked
    const errors = formErrors(fields, { delay: "soon", file: "", note: "" });

    // THEN both are named, each with why, and the third is not
    expect(errors).toEqual({ delay: "not_an_integer", file: "required" });
  });
});

describe("the inputs a launch sends", () => {
  it("sends each value in the type its input declares and leaves out an empty optional one", () => {
    // GIVEN a form filled over every kind of field, one optional left empty
    const fields = [
      field("name", "text"),
      field("body", "long_text"),
      field("file", "file"),
      field("day", "date"),
      field("delay", "integer"),
      field("rate", "number"),
      field("notify", "boolean"),
      field("level", "choice", { choices: ["low", "high"] }),
      field("tags", "list", { itemKind: "text" }),
      field("ids", "list", { itemKind: "integer" }),
      field("extra", "other"),
      field("skipped", "text"),
    ];
    const values = {
      name: " Acme ",
      body: "line one\nline two ",
      file: "/tmp/a.xlsx",
      day: "2026-10-04",
      delay: "7",
      rate: "0.5",
      notify: false,
      level: "high",
      tags: ["a", "b"],
      ids: ["1", "2"],
      extra: '{"k":true}',
      skipped: "",
    };

    // WHEN the inputs are built
    const inputs = toInputs(fields, values);

    // THEN numbers are numbers, the switch is sent off, the JSON is parsed,
    // the long text keeps its lines, and the empty optional one is absent
    expect(inputs).toEqual({
      name: "Acme",
      body: "line one\nline two ",
      file: "/tmp/a.xlsx",
      day: "2026-10-04",
      delay: 7,
      rate: 0.5,
      notify: false,
      level: "high",
      tags: ["a", "b"],
      ids: [1, 2],
      extra: { k: true },
    });
  });
});

describe("a refusal of the workflow engine", () => {
  it("is read back to the input it names, with its reason", () => {
    // GIVEN the message of a refused launch, as either mode of the application carries it
    const embedded = "input 'invoice': no file at /tmp/missing.xlsx [missing_file]";
    const attached = "the Apollia service refused the request (400): input 'delay': expected an integer [wrong_type]";

    // WHEN each is read
    // THEN the input and the reason are found
    expect(parseRefusal(embedded)).toEqual({
      field: "invoice",
      code: "missing_file",
      message: "no file at /tmp/missing.xlsx",
    });
    expect(parseRefusal(attached)).toEqual({
      field: "delay",
      code: "wrong_type",
      message: "expected an integer",
    });
  });

  it("is not invented from an error that names no input", () => {
    // GIVEN an error about the engine, not about an input
    // WHEN it is read
    // THEN no refusal is found
    expect(parseRefusal("the workflow engine is not running in this runtime")).toBeNull();
  });
});

describe("the items of a list", () => {
  it("adds a trimmed item once and ignores a blank one", () => {
    // GIVEN a list holding one item
    // WHEN a new item, the same item and a blank are added
    // THEN only the new one enters
    expect(addItem(["a"], " b ")).toEqual(["a", "b"]);
    expect(addItem(["a"], "a")).toEqual(["a"]);
    expect(addItem(["a"], "  ")).toEqual(["a"]);
  });

  it("checks and unchecks a choice", () => {
    // GIVEN a list holding one choice
    // WHEN another is checked and the first unchecked
    // THEN the list follows
    expect(toggleItem(["x"], "y", true)).toEqual(["x", "y"]);
    expect(toggleItem(["x", "y"], "x", false)).toEqual(["y"]);
    expect(toggleItem(["x"], "x", true)).toEqual(["x"]);
  });
});
