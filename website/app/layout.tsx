import { Provider } from '@/components/provider';
import { basePath } from '@/lib/shared';
import type { Metadata } from 'next';
import './global.css';

export const metadata: Metadata = {
  icons: { icon: `${basePath}/img/logo.png` },
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className="flex flex-col min-h-screen">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
