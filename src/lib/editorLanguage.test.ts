import { describe, expect, it } from "vitest";
import { languageFor } from "./editorLanguage";

describe("languageFor", () => {
  it("knows common code and config files", () => {
    expect(languageFor("deploy.sh")?.name).toBe("Shell");
    expect(languageFor("nginx.conf")?.name).toBe("Nginx");
    expect(languageFor("docker-compose.yml")?.name).toBe("YAML");
    expect(languageFor("Dockerfile")?.name).toBe("Dockerfile");
    expect(languageFor("Cargo.toml")?.name).toBe("TOML");
  });

  it("falls back for shell startup files and ini-style config", () => {
    expect(languageFor(".bashrc")?.name).toBe("Shell");
    expect(languageFor(".profile")?.name).toBe("Shell");
    expect(languageFor("my.cnf")?.name).toBe("Properties files");
    expect(languageFor("poros.service")?.name).toBe("Properties files");
    expect(languageFor(".env.production")?.name).toBe("Properties files");
    expect(languageFor("Dockerfile.dev")?.name).toBe("Dockerfile");
  });

  it("leaves unknown files as plain text", () => {
    expect(languageFor("notes")).toBeNull();
    expect(languageFor("authorized_keys")).toBeNull();
  });
});
