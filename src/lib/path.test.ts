import { describe, expect, it } from "vitest";
import { breadcrumbs, parseHostInput, stemLength } from "./path";

describe("breadcrumbs", () => {
  it("splits POSIX paths from the root", () => {
    expect(breadcrumbs("/home/poros/data", "posix")).toEqual([
      { label: "/", path: "/" },
      { label: "home", path: "/home" },
      { label: "poros", path: "/home/poros" },
      { label: "data", path: "/home/poros/data" },
    ]);
    expect(breadcrumbs("/", "posix")).toEqual([{ label: "/", path: "/" }]);
  });

  it("splits drive letter paths", () => {
    expect(breadcrumbs("C:\\Users\\poros", "windows")).toEqual([
      { label: "C:", path: "C:\\" },
      { label: "Users", path: "C:\\Users" },
      { label: "poros", path: "C:\\Users\\poros" },
    ]);
    expect(breadcrumbs("D:\\", "windows")).toEqual([{ label: "D:", path: "D:\\" }]);
  });

  it("keeps the UNC server and share together", () => {
    expect(breadcrumbs("\\\\server\\share\\reports", "windows")).toEqual([
      { label: "\\\\server\\share", path: "\\\\server\\share\\" },
      { label: "reports", path: "\\\\server\\share\\reports" },
    ]);
  });
});

describe("stemLength", () => {
  it("selects the name without its extension", () => {
    expect(stemLength("report.final.pdf", false)).toBe("report.final".length);
    expect(stemLength(".bashrc", false)).toBe(".bashrc".length);
    expect(stemLength("Makefile", false)).toBe("Makefile".length);
    expect(stemLength("photos.2024", true)).toBe("photos.2024".length);
  });
});

describe("parseHostInput", () => {
  it.each([
    ["example.com", { host: "example.com" }],
    ["  deploy@example.com  ", { host: "example.com", username: "deploy" }],
    ["deploy@example.com:2222", { host: "example.com", username: "deploy", port: 2222 }],
    [
      "sftp://deploy@example.com:2222/var/www",
      { host: "example.com", username: "deploy", port: 2222, path: "/var/www" },
    ],
    ["SSH://example.com", { host: "example.com" }],
    ["user%40corp@example.com", { host: "example.com", username: "user@corp" }],
    ["broken%zz@example.com", { host: "example.com", username: "broken%zz" }],
    ["[2001:db8::1]:2200", { host: "2001:db8::1", port: 2200 }],
    ["2001:db8::1", { host: "2001:db8::1" }],
    ["example.com:notaport", { host: "example.com" }],
  ])("parses %s", (input, expected) => {
    expect(parseHostInput(input)).toEqual({
      username: undefined,
      port: undefined,
      path: undefined,
      ...expected,
    });
  });
});
