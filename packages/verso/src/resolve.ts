export interface IPlatformBinary {
  packageName: string;
  binaryName: string;
}

const supportedPackages = {
  darwin: {
    arm64: "@amamo/verso-darwin-arm64",
    x64: "@amamo/verso-darwin-x64",
  },
  linux: {
    arm64: "@amamo/verso-linux-arm64",
    x64: "@amamo/verso-linux-x64",
  },
  win32: {
    x64: "@amamo/verso-win32-x64",
  },
} as const;

type SupportedPlatform = keyof typeof supportedPackages;

function isSupportedPlatform(platform: string): platform is SupportedPlatform {
  return platform in supportedPackages;
}

export function packageForPlatform(platform: string, arch: string): string {
  if (!isSupportedPlatform(platform)) {
    throw new Error(`Unsupported platform: ${platform} ${arch}`);
  }

  const packageName =
    supportedPackages[platform][arch as keyof (typeof supportedPackages)[SupportedPlatform]];

  if (packageName === undefined) {
    throw new Error(`Unsupported platform: ${platform} ${arch}`);
  }

  return packageName;
}

export function binaryName(platform: string): string {
  return platform === "win32" ? "verso.exe" : "verso";
}

export function resolvePlatformBinary(
  platform: string = process.platform,
  arch: string = process.arch,
): IPlatformBinary {
  return {
    packageName: packageForPlatform(platform, arch),
    binaryName: binaryName(platform),
  };
}
