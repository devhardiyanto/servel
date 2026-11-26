/** Windows dan macOS memakai sistem file yang tidak membedakan huruf besar-kecil. */
const caseInsensitive = /Windows|Mac/i.test(typeof navigator === 'undefined' ? '' : navigator.userAgent)

/** Bentuk pembanding folder: `\` = `/`, tanpa pemisah di akhir. Folder WSL
 * (`\\wsl.localhost\…`) tetap membedakan huruf karena isinya sistem file Linux. */
export function pathKey(p: string): string {
  const n = p.replace(/[\\/]+/g, '/').replace(/\/+$/, '')
  const wsl = /^\/(wsl\.localhost|wsl\$)\//i.test(n)
  return caseInsensitive && !wsl ? n.toLowerCase() : n
}

export function samePath(a: string, b: string): boolean {
  return !!a && !!b && pathKey(a) === pathKey(b)
}
