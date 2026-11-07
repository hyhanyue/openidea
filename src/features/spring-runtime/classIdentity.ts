/** JVM binary names and Java source names can differ at nested type separators. */
export function proxyClass(name: string): boolean {
  return name.includes("$$") || /(?:^|\.)\$Proxy\d+$/.test(name);
}

export function sourceClassName(name: string): string {
  const at = name.lastIndexOf(".") + 1;
  const type = name.slice(at);
  if (proxyClass(name) || type.split("$").some((part) => !part)) return name;
  return name.slice(0, at) + type.replaceAll("$", ".");
}

export function sameClassName(left: string, right: string): boolean {
  return !proxyClass(left) && !proxyClass(right) && sourceClassName(left) === sourceClassName(right);
}
