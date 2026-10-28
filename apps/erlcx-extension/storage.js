export function database() {
  return new Promise((resolve, reject) => {
    const r = indexedDB.open("apex-publishing", 1);
    r.onupgradeneeded = () => r.result.createObjectStore("draft");
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}
export async function storage(mode, value, key = "current") {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(
          "draft",
          mode === "read" ? "readonly" : "readwrite",
        ),
        store = tx.objectStore("draft");
      let result;
      const request =
        mode === "read"
          ? store.get(key)
          : mode === "clear"
            ? store.delete(key)
            : store.put(value, key);
      request.onsuccess = () => {
        result = request.result;
      };
      tx.oncomplete = () => resolve(result);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
  } finally {
    db.close();
  }
}
