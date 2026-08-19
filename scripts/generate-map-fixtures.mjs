import { readFile, mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, join, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..');
const fixturesSourcePath = join(repoRoot, 'src/lib/map/routing-fixtures.ts');
const outputRoot = join(repoRoot, 'test-fixtures/map-routing');

async function loadRoutingFixtures() {
  await readFile(fixturesSourcePath, 'utf8');
  const result = await build({
    entryPoints: [fixturesSourcePath],
    bundle: true,
    write: false,
    format: 'esm',
    platform: 'node',
    packages: 'external',
    target: 'node22',
  });
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`;
  return import(moduleUrl);
}

function renderMarkdown(fixture, document) {
  const titleByPath = new Map(
    fixture.documents.map((item) => [item.path, item.title]),
  );
  const lines = [`# ${document.title}`];

  if (document.links.length > 0) {
    lines.push('', '## References', '');
    for (const targetPath of document.links) {
      const relative =
        posix.relative(posix.dirname(document.path), targetPath) || targetPath;
      const label = titleByPath.get(targetPath) ?? targetPath;
      lines.push(`- [${label}](${relative})`);
    }
  }

  return `${lines.join('\n')}\n`;
}

async function writeFixture(fixture) {
  const fixtureRoot = join(outputRoot, fixture.slug);
  await rm(fixtureRoot, { recursive: true, force: true });
  for (const document of fixture.documents) {
    const filePath = join(fixtureRoot, document.path);
    await mkdir(dirname(filePath), { recursive: true });
    await writeFile(filePath, renderMarkdown(fixture, document), 'utf8');
  }
  return fixture.documents.length;
}

const { ROUTING_FIXTURES } = await loadRoutingFixtures();
let totalDocuments = 0;

for (const fixture of ROUTING_FIXTURES) {
  totalDocuments += await writeFixture(fixture);
}

console.log(
  `Generated ${ROUTING_FIXTURES.length} map routing fixtures (${totalDocuments} documents) under ${outputRoot}`,
);
