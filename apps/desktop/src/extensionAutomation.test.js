import { beforeEach, afterEach, it, expect, vi } from "vitest";
import {
  wizardState,
  permitsAdvance,
} from "../../erlcx-extension/automation.js";
const bundle = {
  title: "Test post",
  description: "Demo description",
  tags: [],
  youtubeUrl: null,
  media: [{}, {}],
  liveries: [
    {
      vehicle: "Falcon",
      primaryColor: "#FFFFFF",
      secondaryColor: null,
      opacity: 100,
      sides: [{ side: "Frontplate 1", decalId: "123" }],
    },
  ],
};
beforeEach(() => {
  vi.stubGlobal("location", new URL("https://erlcx.com/upload"));
  document.body.innerHTML = "<main><button>Continue</button></main>";
});
afterEach(() => vi.unstubAllGlobals());
function fixture(html) {
  document.querySelector("main").innerHTML =
    html +
    "<button>Continue</button><button>Save Draft</button><button>Publish Post</button>";
  document.querySelectorAll("button")[
    document.querySelectorAll("button").length - 1
  ].hidden = true;
}
it("never advances Submit or touches agreement, save, schedule and publish controls", () => {
  fixture(
    '<input id="guidelines" type="checkbox"><input type="datetime-local">',
  );
  const click = vi.fn();
  document
    .querySelectorAll("button,input")
    .forEach((e) => e.addEventListener("click", click));
  expect(wizardState(bundle, true).step).toBe("Submit");
  expect(click).not.toHaveBeenCalled();
  expect(document.querySelector("#guidelines").checked).toBe(false);
  expect(permitsAdvance("Submit")).toBe(false);
});
it("advances only a fully verified Details step", () => {
  fixture(
    '<input id="title"><textarea placeholder="Write your post description"></textarea>',
  );
  const click = vi.fn();
  document.querySelector("button").addEventListener("click", click);
  expect(wizardState(bundle, true).advanced).toBeUndefined();
  document.querySelector("#title").value = bundle.title;
  document.querySelector("textarea").value = bundle.description;
  expect(wizardState(bundle, true).advanced).toBe(true);
  expect(click).toHaveBeenCalledOnce();
});
it("requires the livery-only category and leaves conflicting selections untouched", () => {
  fixture(
    '<button class="border-primary">Liveries Add liveries codes to your post</button><button>Uniforms Add uniforms codes to your post</button>',
  );
  expect(wizardState(bundle).ready).toBe(true);
  document.querySelectorAll("button")[1].classList.add("border-primary");
  expect(wizardState(bundle, true).advanced).toBeUndefined();
});
it("waits for every uploaded image and processing to finish", () => {
  fixture(
    '<input type="file"><p>Images * (2 / 10)</p><img alt="Preview 1"><img alt="Preview 2"><div role="progressbar"></div>',
  );
  document.querySelectorAll("img").forEach((e) => {
    Object.defineProperty(e, "complete", { value: true });
    Object.defineProperty(e, "naturalWidth", { value: 640 });
  });
  expect(wizardState(bundle).ready).toBe(false);
  document.querySelector('[role="progressbar"]').remove();
  expect(wizardState(bundle).ready).toBe(true);
});
it("checks sides, colors, opacity and every decal ID before Codes can continue", () => {
  fixture(
    '<div><input placeholder="Vehicle name (e.g., Crown Victoria)" value="Falcon"><input type="color" value="#ffffff"><input type="number" value="100"><input placeholder="Side (e.g. Left, Right)" value="Frontplate 1"><input placeholder="Decal ID" value="123"><button>Add Side</button></div>',
  );
  expect(wizardState(bundle).ready).toBe(true);
  document.querySelector('input[placeholder="Side (e.g. Left, Right)"]').value =
    "Front 1";
  expect(wizardState(bundle, true).advanced).toBeUndefined();
});
it("only accepts empty monetization and pauses for unknown layouts or CAPTCHA", () => {
  fixture('<input id="robloxGamepassUrl" type="url">');
  expect(wizardState(bundle).ready).toBe(true);
  document.querySelector("input").value = "https://roblox.com/game-pass/123";
  expect(wizardState(bundle).ready).toBe(false);
  fixture("<h2>Different form</h2>");
  expect(wizardState(bundle).step).toBe("");
  fixture('<input id="title"><iframe title="captcha challenge"></iframe>');
  expect(wizardState(bundle).reason).toContain("verification");
});
