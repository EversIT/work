// Invoked by the post-checkout / post-merge git hooks with the list of files
// that changed between the two revs. If package-lock.json is among them, the
// installed node_modules is (probably) stale - print a one-line reminder so a
// `git pull` / branch switch that bumps a dependency doesn't lead to a
// confusing "works on my machine" a day later. Never fails the git operation.
const changedFiles = process.argv.slice(2);

if (changedFiles.includes('package-lock.json')) {
  console.log(
    '\n📦 package-lock.json changed - run `npm ci` to sync your node_modules.\n',
  );
}
