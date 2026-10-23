cd C:\Users\lenovo\Desktop\serverlens
while (-not (Select-String -Path .rename-log.txt -Pattern '^EXIT:' -Quiet -ErrorAction SilentlyContinue)) { Start-Sleep 10 }
cursor-agent -p (Get-Content -Raw .step2-prompt.txt) --force --output-format text *> .step2-log.txt
"EXIT:$LASTEXITCODE" | Add-Content .step2-log.txt
