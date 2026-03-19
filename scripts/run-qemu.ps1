param(
    [string]$LoaderEfiPath
)

$esp = "target/esp"
$bootDir = Join-Path $esp "EFI/BOOT"
$varsPath = "target/OVMF_VARS.fd"

New-Item -ItemType Directory -Force -Path $bootDir | Out-Null
Copy-Item "libs/OVMF_VARS.fd" $varsPath -Force

cargo build -p alchemy-loader --target x86_64-unknown-uefi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo build -p alchemy-kernel `
  --target crates/kernel/x86_64-alchemy-none.json `
  "-Zbuild-std=core,alloc,compiler_builtins" `
  "-Zbuild-std-features=compiler-builtins-mem" `
  "-Zjson-target-spec"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Copy-Item "target/x86_64-unknown-uefi/debug/alchemy-loader.efi" `
          (Join-Path $bootDir "BOOTX64.EFI") -Force

Copy-Item "target/x86_64-alchemy-none/debug/alchemy-kernel" `
          (Join-Path $esp "kernel.elf") -Force

$startup = @"
fs0:
\EFI\BOOT\BOOTX64.EFI
"@
Set-Content -Path (Join-Path $esp "startup.nsh") -Value $startup -NoNewline

qemu-system-x86_64 `
  -drive if=pflash,format=raw,readonly=on,file=libs/OVMF_CODE.fd `
  -drive if=pflash,format=raw,file=$varsPath `
  -net none `
  -vga std `
  -no-reboot `
  -no-shutdown `
  -debugcon stdio `
  -d int,cpu_reset,guest_errors `
  -D target/qemu.log `
  -gdb tcp::1234 `
  -drive format=raw,file=fat:rw:$esp