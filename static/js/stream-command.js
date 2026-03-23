/**
 * Reusable streaming command output via Server-Sent Events.
 *
 * Usage: add these data attributes to a button:
 *   data-stream-command="/admin/upgrade/ytdlp"   — SSE endpoint URL
 *   data-stream-target="output-ytdlp"             — ID of <pre> for output
 *   data-stream-label="Upgrade yt-dlp"            — button label to restore
 */
(function () {
  function initStreamButton(button) {
    button.addEventListener("click", function (e) {
      e.preventDefault();
      var url = button.dataset.streamCommand;
      var targetId = button.dataset.streamTarget;
      var outputEl = document.getElementById(targetId);
      var label = button.dataset.streamLabel || button.textContent;

      button.disabled = true;
      button.textContent = "Running\u2026";
      outputEl.style.display = "block";
      outputEl.textContent = "";

      var source = new EventSource(url);

      source.addEventListener("output", function (ev) {
        outputEl.textContent += ev.data + "\n";
        outputEl.scrollTop = outputEl.scrollHeight;
      });

      source.addEventListener("done", function (ev) {
        var result = JSON.parse(ev.data);
        source.close();
        button.disabled = false;
        button.textContent = label;
        if (result.success) {
          outputEl.textContent += "\n\u2713 Completed successfully\n";
        } else {
          outputEl.textContent +=
            "\n\u2717 Failed (exit code " + result.code + ")\n";
        }
      });

      source.onerror = function () {
        source.close();
        button.disabled = false;
        button.textContent = label;
        outputEl.textContent += "\n\u2717 Connection error\n";
      };
    });
  }

  document
    .querySelectorAll("[data-stream-command]")
    .forEach(initStreamButton);
})();
