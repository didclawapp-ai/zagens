import { defineConfig } from 'astro/config';
import tailwind from '@astrojs/tailwind';
import sitemap from '@astrojs/sitemap';

/** @see https://docs.astro.build/en/guides/internationalization/ */
export default defineConfig({
  site: 'https://zagens.com',
  integrations: [
    tailwind(),
    sitemap({
      i18n: {
        defaultLocale: 'en',
        locales: {
          en: 'en',
          'zh-Hans': 'zh-Hans',
        },
      },
    }),
  ],
});
