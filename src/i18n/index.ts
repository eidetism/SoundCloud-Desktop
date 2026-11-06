import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './en.json';
import ru from './ru.json';

/** Interface language follows the system: Russian on a Russian UI, English otherwise. */
export const language = navigator.language.toLowerCase().startsWith('ru') ? 'ru' : 'en';

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, ru: { translation: ru } },
  lng: language,
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});

document.documentElement.lang = language;

export default i18n;
