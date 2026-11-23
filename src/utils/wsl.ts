/**
 * Nama distro dari folder `\\wsl.localhost\<distro>\…` atau `\\wsl$\<distro>\…`
 * (sama dengan `wsl::parse` di backend). `null` = bukan folder WSL.
 */
export function wslDistro(path: string | undefined | null): string | null {
  const m = /^[\\/]{2}(?:wsl\.localhost|wsl\$)[\\/]([A-Za-z0-9._-]+)(?:[\\/]|$)/i.exec(path ?? '')
  return m ? m[1] : null
}

/** Path Linux dari folder WSL, mis. `/home/dev/app`. */
export function wslLinuxPath(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean)
  return '/' + parts.slice(2).join('/')
}
