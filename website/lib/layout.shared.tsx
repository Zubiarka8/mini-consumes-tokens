import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { appName, basePath, gitConfig } from './shared';

export function baseOptions(): BaseLayoutProps {
  return {
    themeSwitch: { mode: 'light-dark-system' },
    nav: {
      // JSX supported
      title: (
        <span className="flex items-center gap-2">
          <img src={`${basePath}/img/logo.png`} alt="" width={32} height={32} />
          {appName}
        </span>
      ),
    },
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  };
}
