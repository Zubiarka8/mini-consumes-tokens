import Link from 'next/link';

export default function HomePage() {
  return (
    <div className="flex flex-col justify-center text-center flex-1 gap-4 px-4">
      <h1 className="text-3xl font-bold">mini-consumes-tokens</h1>
      <p className="text-fd-muted-foreground max-w-xl mx-auto">
        A tree-sitter symbol graph of your repository in SQLite, served to coding agents over MCP.
      </p>
      <p>
        <Link href="/docs" className="font-medium underline">
          Read the reference manual
        </Link>
      </p>
    </div>
  );
}
