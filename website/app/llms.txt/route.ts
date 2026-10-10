import { docsLlms } from '@/lib/source';
import { withBasePath } from '@/lib/shared';

export const revalidate = false;

export async function GET() {
  return new Response(withBasePath(await docsLlms.index()));
}
