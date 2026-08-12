import { readFileSync } from 'fs';
import { join } from 'path';

export const dynamic = 'force-dynamic';

export default function AdminPage() {
  // Read the admin HTML template
  const htmlPath = join(process.cwd(), 'public', 'admin.html');
  let html = readFileSync(htmlPath, 'utf-8');

  // Replace version placeholder
  const version = process.env.APP_VERSION || '0.4.1';
  html = html.replace(/__VERSION__/g, `v${version}`);

  return (
    <div dangerouslySetInnerHTML={{ __html: html }} />
  );
}
