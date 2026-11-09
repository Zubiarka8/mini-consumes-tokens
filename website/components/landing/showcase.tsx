"use client";
import { useTranslation } from "react-i18next";
import { CopyCommand } from "./copy-command";
import { VaultDemo } from "./vault-demo";
import { WebDemo } from "./web-demo";

type Spec = {
  id: string;
  pre: "web" | "obs";
  command: string;
  codes: string[];
  mock: React.ReactNode;
  flip?: boolean;
};

function Showcase({ id, pre, command, codes, mock, flip }: Spec) {
  const { t } = useTranslation();
  return (
    <section
      id={id}
      className="showcase shell section"
      aria-labelledby={`${pre}-title`}
    >
      <div className={`showcase-top${flip ? " flip" : ""}`}>
        <div className="showcase-copy">
          <p className="eyebrow">{t(`${pre}Label`)}</p>
          <h2 id={`${pre}-title`}>{t(`${pre}Title`)}</h2>
          <p className="body-copy">{t(`${pre}Text`)}</p>
          <CopyCommand command={command} label={t(`${pre}CmdLabel`)} />
        </div>
        <figure className="mock">
          <div className="mock-window">{mock}</div>
          <figcaption>
            <strong>{t("mockTry")}</strong> {t("mockNote")}
          </figcaption>
        </figure>
      </div>
      <div className="feature-grid">
        {[1, 2, 3, 4].map((n) => (
          <article key={n}>
            <span className="mono cap-number">/ 0{n}</span>
            <h3>{t(`${pre}${n}Title`)}</h3>
            <p>{t(`${pre}${n}Text`)}</p>
            <code>{codes[n - 1]}</code>
          </article>
        ))}
      </div>
    </section>
  );
}

const bar = (url: string, live: string) => (
  <div className="mock-bar">
    <span className="mock-dots">
      <i />
      <i />
      <i />
    </span>
    <span className="mock-url">{url}</span>
    <span className="mock-live">{live}</span>
  </div>
);

export function WebShowcase() {
  return (
    <Showcase
      id="web"
      pre="web"
      command="mct-cli --root . web"
      codes={[
        "Dashboard · Statistics",
        "Indexed code · Dependencies",
        "Duplicates · Dead code · Reviews",
        "Search · URL filters",
      ]}
      mock={
        <>
          {bar("127.0.0.1:8787", "live")}
          <WebDemo />
        </>
      }
    />
  );
}

export function ObsidianShowcase() {
  return (
    <Showcase
      id="obsidian"
      pre="obs"
      flip
      command="mct-cli --root . obsidian"
      codes={[
        "title · path · language · symbol_count",
        "100 symbols · 50 imports",
        "init · reindex · mct-mcp-server",
        ".mctignore · .obsidian/",
      ]}
      mock={<VaultDemo />}
    />
  );
}
