# Limites técnicos (sem promessa falsa)

O Ararinha quer abrir **tudo**. Este documento lista, com honestidade, onde a
tecnologia atual impõe limites. Cada limite deve aparecer para o usuário com uma
mensagem clara e uma alternativa.

| Tipo | Como roda | Situação | Limites conhecidos |
|---|---|---|---|
| Linux nativo (`.AppImage`, Flatpak, pacotes Pena) | Direto | Excelente | — |
| `.deb` / `.rpm` | Convertidos ou isolados pelo Bando | Bom | Pacotes que mexem no sistema base são isolados em container |
| Windows `.exe` / `.msi` | Wine + Proton/DXVK/VKD3D | Muito bom | Anti-cheat de kernel (ex.: Vanguard, alguns modos do EA Javelin) não roda; alguns DRMs e drivers de periféricos com app próprio podem falhar |
| Android `.apk` | Waydroid (container Android) | Bom | Apps que exigem certificação do Google (Play Integrity, alguns bancos) podem recusar abrir |
| macOS `.app` / `.dmg` | Darling | **Experimental** | Ferramentas de linha de comando funcionam; apps gráficos, quase nunca. Não distribuímos nenhum componente da Apple. Depois da 1.0, uma camada própria (Fase 6) |
| Windows em ARM64 | FEX-Emu/Box64 + Wine | Experimental | Tradução de CPU custa desempenho |

## Drivers
- Drivers de Windows (`.inf`, `.sys`) **não** funcionam em Linux. Usamos os drivers
  do kernel Linux, firmwares e drivers proprietários oficiais (NVIDIA).
- Alguns periféricos sem driver Linux (certos leitores biométricos, Wi-Fi muito
  novos) podem não funcionar. O Olho de Spix avisa na instalação.

## Secure Boot
- Para bootar com Secure Boot ligado em qualquer PC, precisamos de um `shim`
  assinado pela Microsoft. Isso exige um processo de revisão externo
  (shim-review). Até lá, o usuário pode registrar a nossa chave (MOK) uma única vez.
