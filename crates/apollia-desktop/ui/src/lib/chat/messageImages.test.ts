import { describe, it, expect } from "vitest";
import { dataUrlOf, messageImages, stepIndex } from "./messageImages";

describe("messageImages, the thumbnails a user message shows", () => {
  it("lists the stored images of a reloaded message, in order", () => {
    // GIVEN the metadata the runtime wrote for a message with two images
    const metadata = {
      images: [
        { file: "m-1-1.png", mime_type: "image/png", size: 3 },
        { file: "m-1-2.jpg", mime_type: "image/jpeg", size: 2 },
      ],
    };

    // WHEN its images are read
    const images = messageImages(metadata);

    // THEN both are listed in the order they were sent
    expect(images.map((i) => i.file)).toEqual(["m-1-1.png", "m-1-2.jpg"]);
  });

  it("keeps the composer previews of a message not reloaded yet", () => {
    // GIVEN a message just sent, before the history is read back
    const metadata = {
      images: [{ file: "", mime_type: "image/png", size: 3, preview: "blob:local-1" }],
    };

    // WHEN its images are read
    const images = messageImages(metadata);

    // THEN the preview is what the thumbnail shows
    expect(images).toHaveLength(1);
    expect(images[0]?.preview).toBe("blob:local-1");
  });

  it("lists nothing for a message without images or with a malformed entry", () => {
    // GIVEN a plain message and a malformed list
    // WHEN their images are read
    // THEN nothing is listed
    expect(messageImages(null)).toEqual([]);
    expect(messageImages({ thinking_trace: "x" })).toEqual([]);
    expect(messageImages({ images: [{ file: 3 }] })).toEqual([]);
  });

  it("builds a data URL from a stored image and wraps the viewer position", () => {
    // GIVEN a stored image and a viewer on two images
    const url = dataUrlOf({ mime_type: "image/png", data: "AQID" });

    // WHEN the viewer steps past either end
    // THEN the URL carries the type and bytes, and the position wraps
    expect(url).toBe("data:image/png;base64,AQID");
    expect(stepIndex(1, 2, 1)).toBe(0);
    expect(stepIndex(0, 2, -1)).toBe(1);
  });
});
