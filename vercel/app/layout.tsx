import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: '微信服务管理后台',
  description: 'WeChat Official Account Backend Management Dashboard',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN">
      <body>{children}</body>
    </html>
  );
}
