"use client";
import { createInstance } from "i18next";
import { I18nextProvider, initReactI18next } from "react-i18next";
import { useEffect, useState, type ReactNode } from "react";
import en from "@/messages/en";
import es from "@/messages/es";

export const localeKey = "mct-home-language";
export function LandingLocale({ children }: { children: ReactNode }) {
  const [i18n] = useState(() => {
    const instance = createInstance();
    void instance.use(initReactI18next).init({
      resources: { en: { translation: en }, es: { translation: es } },
      lng: "en",
      fallbackLng: "en",
      supportedLngs: ["en", "es"],
      initAsync: false,
      interpolation: { escapeValue: false },
      react: { useSuspense: false },
    });
    return instance;
  });
  useEffect(() => {
    const previous = document.documentElement.lang;
    const update = (language: string) => {
      document.documentElement.lang = language;
    };
    i18n.on("languageChanged", update);
    try {
      const saved = localStorage.getItem(localeKey);
      if (saved === "en" || saved === "es") void i18n.changeLanguage(saved);
    } catch {
      /* The English default also works when storage is unavailable. */
    }
    update(i18n.language);
    return () => {
      i18n.off("languageChanged", update);
      document.documentElement.lang = previous;
    };
  }, [i18n]);
  return <I18nextProvider i18n={i18n}>{children}</I18nextProvider>;
}
