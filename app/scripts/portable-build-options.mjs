export function qaBuild(args) {
  if (args.includes('--all-features')) return true;
  const features = args.flatMap((arg, index) => ['--features', '-F'].includes(arg)
    ? (args[index + 1] ?? '').split(/[ ,]+/)
    : arg.startsWith('--features=') ? arg.slice(11).split(/[ ,]+/)
    : arg.startsWith('-F') ? arg.slice(2).replace(/^=/, '').split(/[ ,]+/) : []);
  return features.some(feature => /(?:^|\/)qa-(?:harness|webdriver)$/.test(feature));
}
