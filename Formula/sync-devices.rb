class SyncDevices < Formula
  desc "Cross-platform CLI tool for syncing AI CLI tool configurations across devices"
  homepage "https://github.com/user/sync-devices"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/user/sync-devices/releases/download/v#{version}/sync-devices-darwin-aarch64"
      sha256 "PLACEHOLDER_SHA256_DARWIN_AARCH64"
    else
      url "https://github.com/user/sync-devices/releases/download/v#{version}/sync-devices-darwin-x86_64"
      sha256 "PLACEHOLDER_SHA256_DARWIN_X86_64"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/user/sync-devices/releases/download/v#{version}/sync-devices-linux-aarch64"
      sha256 "PLACEHOLDER_SHA256_LINUX_AARCH64"
    else
      url "https://github.com/user/sync-devices/releases/download/v#{version}/sync-devices-linux-x86_64"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    binary_name = "sync-devices"
    # The downloaded file is the raw binary
    downloaded = Dir["*"].first || binary_name
    bin.install downloaded => binary_name
  end

  test do
    assert_match "sync-devices", shell_output("#{bin}/sync-devices --version")
  end
end
