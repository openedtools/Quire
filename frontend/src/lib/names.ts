/**
 * Fuzzy matching of people's names, so typing "Professor Smith" points at an existing "Prof Smith"
 * instead of quietly creating a duplicate. Ignores case, accents and punctuation.
 */

export function normalizeName(name: string): string {
  return name
    .normalize('NFD')
    .replace(/\p{M}+/gu, '')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();
}

function editDistance(a: string, b: string): number {
  let previous = Array.from({ length: b.length + 1 }, (_, j) => j);
  for (let i = 1; i <= a.length; i++) {
    const current = [i];
    for (let j = 1; j <= b.length; j++) {
      current[j] = Math.min(
        previous[j] + 1,
        current[j - 1] + 1,
        previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
    }
    previous = current;
  }
  return previous[b.length];
}

/** Two words that are the same, one the start of the other ("prof"/"professor"), or a typo apart. */
function wordsAlike(a: string, b: string): boolean {
  if (a === b) return true;
  const shortest = Math.min(a.length, b.length);
  if (shortest >= 3 && (a.startsWith(b) || b.startsWith(a))) return true;
  return shortest >= 4 && editDistance(a, b) <= 1;
}

/** 1 for the same name, roughly 0.75 to 0.9 for a close variant, 0 for unrelated names. */
export function nameSimilarity(a: string, b: string): number {
  const x = normalizeName(a);
  const y = normalizeName(b);
  if (!x || !y) return 0;
  if (x === y) return 1;

  const wordsX = x.split(' ');
  const wordsY = y.split(' ');
  const [short, long] = wordsX.length <= wordsY.length ? [wordsX, wordsY] : [wordsY, wordsX];

  // Every word of the shorter name matches a different word of the longer one:
  // "prof smith" ~ "professor smith", "smith" ~ "prof smith", "dir osei" ~ "director osei"
  const taken = new Set<number>();
  const allWordsMatch = short.every((word) => {
    const at = long.findIndex((other, index) => !taken.has(index) && wordsAlike(word, other));
    if (at < 0) return false;
    taken.add(at);
    return true;
  });
  if (allWordsMatch) return short.length === long.length ? 0.9 : 0.8;

  // Otherwise a small spelling difference over the whole name ("jon smith" ~ "john smith")
  const whole = 1 - editDistance(x, y) / Math.max(x.length, y.length);
  return whole >= 0.8 ? whole : 0;
}

/** Existing people whose names resemble `query` but are not the same name, closest first. */
export function similarPeople<T extends { name: string }>(query: string, people: T[], limit = 3): T[] {
  return people
    .map((person) => ({ person, score: nameSimilarity(query, person.name) }))
    .filter(({ score }) => score >= 0.75 && score < 1)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map(({ person }) => person);
}
