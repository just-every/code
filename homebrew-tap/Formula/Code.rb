class Code < Formula
  desc "Terminal coding agent"
  homepage "https://github.com/just-every/code"
  version "0.4.7"
  livecheck do
    url :homepage
    strategy :github_latest
  end
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/just-every/code/releases/download/v0.4.7/code-aarch64-apple-darwin.tar.gz"
      sha256 "76ac9be154c86193bd2f66ddc7bc82168f310574a3ebbdc279548d91d232e2a6"
    else
      url "https://github.com/just-every/code/releases/download/v0.4.7/code-x86_64-apple-darwin.tar.gz"
      sha256 "5968dc7bf681ccb7fb9a825cfbfb33870e2c249f2ca326e8a70c7f4cedfd63e7"
    end
  end

  def install
    bin.install Dir["code-*"].first => "code"
    # Provide a compatibility shim
    (bin/"coder").write <<~EOS
      #!/bin/bash
      exec "#{bin}/code" "$@"
    EOS
  end

  test do
    system "#{bin}/code", "--help"
  end
end
