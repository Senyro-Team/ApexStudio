export function nativeConnection() {
  const port = chrome.runtime.connectNative("com.apexstudio.erlcx");
  let pending = null,
    closed = false,
    chain = Promise.resolve();
  port.onMessage.addListener((response) => {
    if (!pending) return;
    const p = pending;
    pending = null;
    clearTimeout(p.timer);
    if (response.error) {
      const error = Error(response.error);
      error.cancelled = response.cancelled === true;
      p.reject(error);
    } else p.resolve(response);
  });
  port.onDisconnect.addListener(() => {
    closed = true;
    const message =
      chrome.runtime.lastError?.message || "Connection to Apex Studio closed.";
    if (pending) {
      clearTimeout(pending.timer);
      pending.reject(Error(message));
      pending = null;
    }
  });
  return {
    request(message) {
      const action = () =>
        new Promise((resolve, reject) => {
          if (closed) {
            reject(Error("Connection to Apex Studio closed."));
            return;
          }
          pending = {
            resolve,
            reject,
            timer: setTimeout(() => {
              pending = null;
              reject(Error("Apex Studio did not respond."));
              port.disconnect();
            }, 20000),
          };
          port.postMessage(message);
        });
      const result = chain.then(action);
      chain = result.catch(() => {});
      return result;
    },
    close() {
      port.disconnect();
    },
  };
}
