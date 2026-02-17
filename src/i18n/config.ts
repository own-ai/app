import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";

import enTranslation from "@/locales/en/translation.json";
import deTranslation from "@/locales/de/translation.json";

i18n
  // Detect user language
  .use(LanguageDetector)
  // Pass the i18n instance to react-i18next
  .use(initReactI18next)
  // Init i18next
  .init({
    resources: {
      en: {
        translation: enTranslation,
      },
      de: {
        translation: deTranslation,
      },
    },
    fallbackLng: "en",
    debug: false,

    interpolation: {
      escapeValue: false, // React already escapes values
    },

    detection: {
      // Order of detection methods
      order: ["localStorage", "navigator"],
      // Cache user language
      caches: ["localStorage"],
      // LocalStorage key
      lookupLocalStorage: "ownai_language",
    },
  });

export default i18n;
