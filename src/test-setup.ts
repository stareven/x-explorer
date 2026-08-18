import "@testing-library/jest-dom";

// Under Vitest 4 + jsdom + Node 22+, `globalThis.localStorage` resolves to
// `node:internal/webstorage`, which has no backing file and emits a spurious
// "--localstorage-file was provided without a valid path" warning on first
// use. src/store/index.ts reads localStorage synchronously at module init
// (see loadJson), so the warning fires once per test worker. Replace the
// proxy with a working Map-backed stub. `instanceof Storage` is safe to call
// here because it does not invoke the proxy.
const ls = (globalThis as { localStorage?: unknown }).localStorage;
if (!(ls instanceof Storage)) {
  const data = new Map<string, string>();
  const stub = {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => {
      data.set(key, String(value));
    },
    removeItem: (key: string) => {
      data.delete(key);
    },
    clear: () => {
      data.clear();
    },
    key: (index: number) => Array.from(data.keys())[index] ?? null,
  };
  Object.defineProperty(stub, "length", {
    enumerable: true,
    get: () => data.size,
  });
  Object.defineProperty(
    (typeof window !== "undefined" ? window : globalThis) as object,
    "localStorage",
    { value: stub, writable: true, configurable: true },
  );
}
