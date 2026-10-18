const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const workspaceRoot = path.join(__dirname, '..');

/** Directory names under `apps/` or `packages/` - re-read every run, so a new app/package shows up automatically without editing this file. */
function projectNames(relDir) {
  const abs = path.join(workspaceRoot, relDir);
  if (!fs.existsSync(abs)) return [];
  return fs
    .readdirSync(abs, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && !entry.name.startsWith('.'))
    .map((entry) => entry.name)
    .sort();
}

function projectScopes(relDir) {
  return projectNames(relDir).map((name) => ({
    value: name,
    name: `${name.padEnd(28)}${relDir}/${name}`,
  }));
}

// Repo-wide scopes that don't map to a single app/package.
// prettier-ignore
const fixedScopes = [
  { value: 'core', name: 'core                        cross-cutting change, not specific to one app/package (if unsure, use this)' },
  { value: 'repo', name: 'repo                        tooling/workspace config - nx.json, scripts/, workflows, Dockerfiles, ...' },
  { value: 'docs', name: 'docs                        documentation only (READMEs, etc.)' },
  { value: 'deps', name: 'deps                        dependency bumps' },
];

const scopes = [
  ...fixedScopes,
  ...projectScopes('apps'),
  ...projectScopes('packages'),
];

// Precomputed default scope: look at whatever's changed (staged or not) and
// pull the app/package name out of the first apps/<name> or packages/<name>
// path it touches. Anchored to the actual file path (not just "contains
// apps/ or packages/ somewhere"), otherwise unrelated paths that happen to
// have "packages/" in a directory name (e.g. a
// .cursor/skills/link-workspace-packages/SKILL.md) false-match.
const scopeComplete = (() => {
  let status;
  try {
    status = execSync('git status --porcelain').toString();
  } catch {
    return undefined;
  }
  for (const line of status.split('\n')) {
    const filePath = line.slice(3).split(' -> ').pop();
    const match = filePath?.match(/^(?:apps|packages)\/([^/]+)/);
    if (match) return match[1];
  }
  return undefined;
})();

/** @type {import('cz-git').CommitizenGitOptions} */
module.exports = {
  /** @usage `npm run commit :f` */
  alias: {
    f: 'docs(core): fix typos',
    b: 'chore(repo): bump dependencies',
  },
  scopes,
  defaultScope: scopeComplete,
  scopesSearchValue: true,
  maxSubjectLength: 100,
  allowCustomScopes: false,
  allowEmptyScopes: false,
  allowCustomIssuePrefix: false,
  allowEmptyIssuePrefix: false,
  types: [
    { value: 'feat', name: 'feat:     A new feature' },
    { value: 'fix', name: 'fix:      A bug fix' },
    { value: 'docs', name: 'docs:     Documentation only changes' },
    {
      value: 'cleanup',
      name: 'cleanup:  A code change that neither fixes a bug nor adds a feature',
    },
    {
      value: 'chore',
      name: "chore:    Other changes that don't modify src or test files",
    },
  ],
};
