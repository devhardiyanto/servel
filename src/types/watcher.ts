export interface VersionFileDetected {
  version: string
  path: string
}

export type PhpvmrcDetected = VersionFileDetected
export type NvmrcDetected = VersionFileDetected
