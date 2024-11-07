.PHONY: decomp
decomp:
	rm -rf /tmp/considition2024 || true
	mkdir -p /tmp/considition2024
	nix run 'nixpkgs#skopeo' copy docker://sywor/considition2024:latest dir://tmp/considition2024
	cd /tmp/considition2024 && file * | rg '([0-9a-f]{64}): gzip' -o -r '$$1' | xargs -I{} tar xvf {}
