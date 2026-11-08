const step = document.getElementById("step");
const status = document.getElementById("status");
let started = false;

window.apexHandoffStarted = () => {
  started = true;
  clearTimeout(startupTimer);
};

document.getElementById("handoff-module").addEventListener("error", () => {
  step.textContent = "Extension failed to load";
  status.textContent =
    "Apex Studio’s handoff script could not load. Reload the extension and try again.";
});

const startupTimer = setTimeout(() => {
  if (started) return;
  step.textContent = "Extension failed to start";
  status.textContent =
    "The handoff page did not start. Check the extension files and reload Apex Studio’s extension.";
}, 8000);
