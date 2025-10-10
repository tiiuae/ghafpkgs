{
  lib,
  buildGoModule,
}:
buildGoModule {
  pname = "swtpm-proxy";
  version = "0.0.1";
  src = ./.;
  vendorHash = "sha256-s1oLreAT112iz7NP/KKKjjlBKNnclmmfj/3nIZuKnSA="; 
  subPackages = [ "cmd/swtpm-proxy" ];

  overrideModAttrs = old: {
    buildFlags = [ "-mod=mod" ];
  };

  meta = {
    description = "A proxy for swtpm written in Go.";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}