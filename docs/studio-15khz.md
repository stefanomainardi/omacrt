# Studio di fattibilità: Omarchy su CRT a 15 kHz

Data: 2026-09-05. Obiettivo: collegare un PC Omarchy a un televisore CRT via SCART RGB a 15 kHz nativi, senza compromessi: risoluzioni native per gioco (224p, 240p, 256p, 288p), refresh esatti (60.00, 59.94, 57.5, 50 Hz), 480i e 576i interlacciati, cambio modo automatico a ogni gioco. Il tutto integrato nello stile di Omarchy.

## 1. Sintesi

La cosa è fattibile e la comunità GroovyArcade/Batocera-CRT la fa da anni su Arch. Il progetto si riduce a quattro blocchi, ognuno con una scelta chiara:

| Blocco                 | Scelta                                                                                                        | Motivo                                                                                                        |
| ---------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| DAC digitale/analogico | Adattatore DisplayPort→VGA con chip Realtek RTD2166/RTD2168                                                   | Unico chip documentato che accetta pixel clock da ~8 MHz. HDMI escluso.                                       |
| Sync e SCART           | VideoAmp (preferito) oppure UMSA / sirMagb F-15                                                               | Serve un combinatore H+V→csync attivo, livello 0.3-1 V, e 1-3 V sul pin 16 SCART.                             |
| Kernel                 | `linux-lts` 6.18 ricompilato con le patch 15 kHz di D0023R                                                    | amdgpu vanilla rifiuta i modi a basso dot clock e non gestisce bene l'interlace. RDNA3 è coperto dalle patch. |
| Modesetting            | Hyprland con modeline fissa per il "desktop CRT"; RetroArch/GroovyMAME in KMS/DRM su VT dedicato per il gioco | Wayland non permette il cambio modo per gioco. KMS diretto sì.                                                |

Il punto più delicato non è l'hardware ma il kernel: Omarchy avvia via Limine con Unified Kernel Image e l'hook `limine-mkinitcpio-hook` genera una voce di boot per ogni kernel installato. Un pacchetto kernel aggiuntivo si integra quindi in modo pulito, ma va ricompilato a ogni aggiornamento della serie LTS.

## 2. Hardware rilevato sulla macchina

Rilevato il 2026-09-05 con `lspci`, `pacman -Q`, `/sys/class/drm` e `/boot/limine.conf`.

- **GPU discreta**: AMD Navi 32 (Radeon RX 7700 XT / 7800 XT), driver `amdgpu`, `card0`. Connettori: DP-4, DP-5 (entrambi in uso), HDMI-A-2 libero.
- **GPU integrata**: AMD Granite Ridge (Ryzen 9000), driver `amdgpu`, `card1`. Connettori: DP-1, DP-3, HDMI-A-1 liberi, DP-2 in uso.
- **Kernel**: `linux` 7.2.3.arch1-2 installato (7.1.9 in esecuzione, riavvio pendente) e `linux-lts` 6.18.49-2.
- **Boot**: Limine, UKI in `/boot/EFI/Linux/omarchy_linux.efi` e `omarchy_linux-lts.efi`, configurazione in `/etc/default/limine` con `ENABLE_UKI=yes`. Root cifrata su btrfs con snapshot.
- **Hyprland**: 0.56.2 su aquamarine.
- **Omarchy**: 4.0.2, 428 script in `~/.local/share/omarchy/bin`, tra cui la famiglia `omarchy-hyprland-monitor-*` e `omarchy-refresh-limine`.
- **TV**: Bang & Olufsen BeoCenter 1 con ingresso SCART RGB, già validato a 60 Hz con RGB-Pi su Raspberry Pi 4.

Nota sulla scelta della GPU: entrambe le schede hanno porte DisplayPort libere. Usare l'iGPU per il CRT terrebbe la dGPU libera per il desktop, ma la lista di compatibilità Batocera copre esplicitamente RDNA3 discrete (RX 7600/7700 XT/7800 XT/7900) e le APU Phoenix e Strix Point, non Granite Ridge. La strada più sicura è la dGPU, liberando una delle due porte DisplayPort (HDMI non va bene per il CRT, ma va benissimo per un monitor desktop). Vedi sezione 3.1.

## 3. Catena del segnale

```
GPU (DP) → DAC RTD2166 (VGA RGBHV) → combinatore sync + blanking (SCART RGBS) → BeoCenter 1
```

### 3.1 DAC: perché solo DisplayPort e solo RTD2166/2168

Le GPU moderne non hanno più uscite analogiche. L'ultima AMD con DVI-I analogico è la R9 380X. Serve un convertitore attivo e quasi tutti rifiutano i pixel clock sotto ~25 MHz che servono per 320x240 a 15 kHz (circa 6-7 MHz).

Il chip Realtek RTD2166 (e il successore RTD2168) accetta pixel clock da circa 8 MHz a 210 MHz, non ha deinterlacer automatico e non dipende da un quarzo proprio. È il chip raccomandato da GroovyArcade e dal progetto Batocera-CRT-Script. Modelli documentati:

- **CableDeconn DP to VGA**, versione non 4K e senza audio. Fornisce 5 V sul pin 9 VGA, utile per alimentare adattatori passivi. Le varianti 4K o con audio non hanno il chip giusto.
- **Cable Matters DP to VGA, modello 102026**. Solo ~3.2 V sul pin 9.
- **biaze DP to VGA, modello ZH277**. Solo ~3.2 V sul pin 9.

Il pin 9 conta solo se il combinatore sync viene alimentato dal VGA. UMSA e VGA2SCART si alimentano via USB, quindi il problema non si pone.

**HDMI è esplicitamente non supportato** dal percorso amdgpu 15 kHz: il TMDS HDMI ha un minimo di 25 MHz e il driver non permette modi sotto quella soglia su HDMI. Le porte da usare sono quindi DP-4 o DP-5 della dGPU, oggi occupate dai monitor. Opzioni: spostare un monitor sull'HDMI-A-2 della dGPU (i monitor desktop non hanno il problema del dot clock), oppure usare l'iGPU (DP-1 o DP-3) accettando che non sia in lista di compatibilità.

Il cambio risoluzione attraverso il DAC è documentato come leggermente più lento rispetto a un'uscita VGA nativa, ma funziona anche durante il gioco.

### 3.2 Sync e SCART

Un CRT consumer via SCART vuole:

- RGB a 0.7 Vpp su pin 15, 11, 7, che il DAC fornisce già.
- **Sync composito** (csync) a 0.3-1 V sul pin 20. Il VGA emette H e V separati a livello TTL: vanno combinati e attenuati. Un sync TTL a 5 V diretto è fuori specifica.
- **Blanking RGB**: 1-3 V sul pin 16, altrimenti la TV resta in composito o mostra bianco e nero.
- **Commutazione**: 9.5-12 V sul pin 8 per far passare la TV all'ingresso AV automaticamente. Comodo, non obbligatorio.

Soluzioni raccomandate dal wiki Batocera-CRT-Script, in ordine di preferenza:

1. **VideoAmp** (njz3 e Bandicoot): amplificatore VGA con combinatore csync, protezione contro frequenze fuori range, uscita SCART RGB, audio, uscita sync per GunCon e **emulatore EDID configurabile**. Quest'ultimo punto risolve il problema dell'EDID assente (sezione 5.3). È il pezzo più completo e il più adatto a questo progetto.
2. **sirMagb F-15**, progetto open su Hackaday, evoluzione dello Scart Vader con sync migliorato.
3. **UMSA Ultimate SCART Adapter** di Arcadeforge: classico, combinatore su chip logico, alimentazione 5 V 250 mA via USB per pin 8 e pin 16, audio via due RCA.
4. **VGA2SCART** di Retro Upgrades: alimentazione USB, fornisce il pin 16.

Da evitare: cavi VGA→SCART passivi (nessuna schermatura, sync debole), Scart Vader e le versioni Arcade Express (PIC lento nel riagganciare il sync), adattatore di Tim Worthington (blocca il sync quando il segnale è fuori specifica e fa commutare la TV), cavi SCART per MiSTer (aspettano 3.3-5 V sul pin 9 e csync TTL sul pin 13: incompatibili con un VGA PC).

### 3.3 Alternativa scartata: DAC HDMI a modo fisso

Esistono DAC HDMI→SCART pensati per l'emulazione: **HDMI2SCART** di c0pperdragon e **RGB-Pi 2** (24 bit, sync AND/XOR selezionabile, uscita sync 3.5 mm per light gun, circa 70 USD). Espongono un EDID fisso (HDMI2SCART: 1440x240 o 1440x288 secondo l'interruttore 50/60) e convertono qualunque sorgente HDMI. Vantaggio enorme: nessuna patch kernel, la GPU vede un monitor normale a 27 MHz.

Sono però incompatibili con l'obiettivo "nessun compromesso": una sola risoluzione verticale, un solo refresh, niente 480i, niente 57.5 Hz arcade. Restano una buona **modalità di ripiego** o un primo prototipo a spesa ridotta. Per RGB-Pi 2 una nota RetroRGB del 2026-03-31 segnala problemi di sync e video nei test, da verificare.

## 4. Kernel: le patch 15 kHz

### 4.1 Cosa fanno

Il repository `D0023R/linux_kernel_15khz` mantiene aggiornate le patch di Calamity (autore di GroovyMAME e Switchres). Per la serie 7.1 le patch sono otto:

1. `01_linux_15khz.patch`: rimuove i limiti minimi di dot clock e abilita i modi 15/25/31 kHz in DRM.
2. `02_linux_15khz_interlaced_mode_fix.patch`: correzione generale dell'interlace.
3. `03_linux_15khz_dcn1_dcn2_dcn3_dcn4_interlaced_mode_fix.patch`: interlace per i display engine DCN 1-4 (RDNA1, 2, 3, 4 e APU).
4. `04_linux_15khz_dce_interlaced_mode_fix.patch`: interlace per i vecchi DCE.
5. `05_linux_15khz_amdgpu_pll_fix.patch`: calcolo PLL corretto per clock bassi.
6. `06_linux_switchres_kms_drm_modesetting.patch`: permette a Switchres di aggiungere modi utente via ioctl senza Xorg.
7. `07_linux_15khz_fix_ddc.patch`: DDC.
8. `08_linux_15khz_interlace_force_even.patch`: forza linee pari nei modi interlacciati.

Versioni mantenute al 2026-09-05: stable 7.2.2 e 7.1.8, LTS 6.18.45, 6.12.104, 6.6.152, 6.1.183. La serie 6.18 è quella di `linux-lts` su Arch, quindi le patch seguono esattamente il kernel che Omarchy già installa come secondo kernel.

Parametri di boot: `video=<connettore>:<modo>` con suffissi `e` (forza il connettore attivo anche senza EDID), `S` (abilita i modi a basso dot clock), `i` (interlacciato). Esempi: `video=DP-2:320x240eS`, `video=DP-2:640x480ieS`.

### 4.2 Copertura RDNA3

Il wiki Batocera-CRT-Script elenca RDNA3 (Navi 3x, DCN 3.2) come confermato per uscita CRT nativa, citando RX 7600, 7700 XT, 7800 XT, 7900 XT/XTX. Le APU Phoenix (DCN 3.1.4) e Strix Point (DCN 3.5) sono supportate. Non trovata conferma esplicita per Granite Ridge. Polaris è l'unica generazione recente con esito misto, e non ci riguarda.

### 4.3 Integrazione con Omarchy

Omarchy non documenta un percorso ufficiale per kernel alternativi (discussione omacom/omarchy #3700 ancora aperta), ma la configurazione locale mostra che l'infrastruttura c'è: `/etc/default/limine` con `ENABLE_UKI=yes` e voci auto-generate da `limine-entry-tool` per `linux` e `linux-lts`. Un terzo pacchetto kernel produrrebbe la sua UKI e la sua voce.

Strategia proposta:

- Pacchetto **`linux-crt`** costruito dal PKGBUILD di `linux-lts` (ABS) con le otto patch applicate e `pkgbase` rinominato, così convive con `linux` e `linux-lts` senza conflitti.
- Voce Limine dedicata "Omarchy CRT" con cmdline che aggiunge `video=DP-x:...eS` e `drm.edid_firmware=...`. Il kernel stock resta la voce di default: il desktop quotidiano non cambia.
- Rebuild automatico quando esce una nuova `linux-lts`: script che scarica il PKGBUILD, applica le patch della cartella corrispondente e ricompila. Compilazione lunga (ordine dell'ora su questa CPU), da mettere in background.
- Alternativa: un repository pacman personale su GitHub/GitLab con il pacchetto già compilato, aggiornato da CI. Più comodo per altri utenti Omarchy, che è lo scopo finale del progetto.

Rischio da tenere presente: le patch modificano `drivers/gpu/drm/amd/display`. Se un aggiornamento LTS cambia quelle aree, le patch possono fallire fino all'aggiornamento del repository D0023R. La cadenza storica del repository è buona (segue le stable entro giorni).

## 5. Modesetting: Hyprland e KMS

### 5.1 Cosa può fare Hyprland

Verificato nel sorgente `main` di Hyprland (`src/config/shared/monitor/Parser.cpp`) e di aquamarine (`src/backend/drm/DRM.cpp`) il 2026-09-05:

- Hyprland accetta modeline personalizzate nella regola monitor: `monitor = DP-2, modeline 6.400 320 336 368 400 240 244 247 262 -hsync -vsync, 0x0, 1`. Il parser legge clock e otto valori di timing, poi i flag.
- **Bug sull'interlace**: la mappa dei flag contiene la chiave `Interlace` con la maiuscola, ma il parser converte ogni flag in minuscolo prima di cercarlo. Il flag non viene mai riconosciuto, viene loggato come "Invalid flag" e il modo viene applicato come progressivo con timing sbagliati. Coerente con l'issue #4607 chiusa come "not planned" nel 2024.
- aquamarine scarta esplicitamente ogni modo interlacciato letto dall'EDID ("Skipping mode ... because it's interlaced"). I modi personalizzati passano per `customMode`, ma con il bug sopra il flag non arriva.
- aquamarine non impone un minimo di risoluzione o di clock: un 320x240 progressivo passa al driver, che decide con o senza patch.

Conclusione: sotto Hyprland si possono avere modi **progressivi fissi** a 15 kHz (240p, 288p, super-risoluzioni tipo 2560x240) ma **niente interlace** e **niente cambio modo per gioco**. L'issue #102 di Switchres conferma il limite generale di Wayland: il protocollo `wlr-output-management` accetta solo larghezza, altezza e refresh, e non esiste uno standard per modeline arbitrarie.

Un fix upstream per il bug del flag è un contributo piccolo e utile al progetto Hyprland, indipendente da tutto il resto. Anche con il fix, il cambio modo per gioco resterebbe fuori portata sotto compositor.

### 5.2 Cosa serve per il gioco: KMS/DRM diretto

RetroArch e GroovyMAME integrano Switchres come libreria. In modalità KMS/DRM l'emulatore diventa DRM master e applica lui la modeline calcolata da Switchres, senza compositor. Il maintainer di GroovyArcade ha confermato nell'agosto 2026 (PR RetroArch #17353) che "KMS/DRM modeswitching works perfectly" dal marzo 2023 e che richiede il kernel patchato con la patch 06 per i modi utente via ioctl.

Requisiti pratici:

- Kernel con patch 06.
- L'emulatore deve essere DRM master sul device del CRT. Con Hyprland attivo sulla stessa scheda serve passare a un altro VT (`chvt`) e lanciare lì l'emulatore, oppure usare il device dell'altra GPU. Il primo è il modello classico di "Big Picture": Hyprland resta vivo sul VT 1, il gioco gira sul VT 2.
- Switchres in modalità drmkms richiede privilegi root o una regola sudoers mirata (discusso nell'issue #102).
- `switchres.ini` con profilo monitor `ntsc`, `pal` o `generic_15` e con `dotclock_min 0` (il DAC RTD supporta i clock bassi, non serve il ripiego a 25 MHz).
- RetroArch: `crt_switch_resolution = 1` in modalità nativa, video driver `kms`. GroovyMAME: `-video drmkms` o SDL con `switchres` attivo, da verificare sulla build Arch corrente.

Questa architettura dà tutto ciò che l'obiettivo richiede: risoluzioni native, refresh esatti, 480i, nessun frame di scaling.

### 5.3 EDID assente

Una TV SCART non ha EDID e il DAC RTD si limita a passare quello del monitor collegato: il kernel vede il connettore "disconnected". Due rimedi complementari:

- Parametro `video=DP-x:320x240eS`: la `e` forza il connettore attivo.
- `drm.edid_firmware=DP-x:edid/crt15.bin` con un EDID costruito ad hoc, copiato in `/usr/lib/firmware/edid/` e incluso nell'initramfs (quindi nella UKI, con `mkinitcpio` FILES). Switchres sa generare EDID; gli esempi Batocera li generano dal profilo monitor.
- Il VideoAmp emula un EDID lato hardware con risoluzioni 15 kHz e super-risoluzioni: elimina il problema alla radice e rende il connettore hot-plug normale.

## 6. Disegno del progetto `omarchy-crt`

Stile Omarchy: script bash con prefisso, voce nel menu (`omarchy-menu`), colori dal tema corrente, nessuna GUI pesante.

### 6.1 Componenti

- **`omarchy-crt-install`**: verifica GPU amdgpu e connettore DP libero, installa `linux-crt` (dal repo pacman del progetto o compilando), installa `switchres`, `retroarch`, `groovymame`, scrive `/etc/default/limine` con la voce "Omarchy CRT", genera EDID e `switchres.ini` dal profilo scelto (NTSC, PAL, generic 15), aggiunge la regola sudoers per switchres. Idempotente, con dry run.
- **`omarchy-crt-detect`**: rileva connettore CRT e stato del kernel (patchato o no), usato dagli altri script e dal menu.
- **`omarchy-crt-desktop`**: attiva sotto Hyprland una modeline 15 kHz progressiva sul connettore CRT (240p o 288p, o super-risoluzione 2560x240), sposta un workspace dedicato, applica font e scala per la bassa risoluzione, mostra lo splash. Serve come "salotto" prima del gioco e per la demo visiva.
- **`omarchy-crt-play`**: passa al VT 2 e lancia RetroArch o GroovyMAME in KMS con Switchres, poi torna a Hyprland all'uscita. Gestisce audio (PipeWire sul VT secondario), gamepad e wake dei monitor.
- **`omarchy-crt-splash`**: animazione 240p all'avvio della modalità, logo Omarchy con la palette del tema corrente, stile boot console anni '90. Realizzabile con un video 320x240 pre-renderizzato per tema (mpv) o una piccola app SDL che legge i colori da `~/.config/omarchy/current/theme`.
- **Voce menu**: "Gaming → CRT" con sottovoci Desktop CRT, Gioca (RetroArch), Gioca (GroovyMAME), Profilo TV (NTSC/PAL), Test pattern.
- **Test pattern**: schermate 240p e 480i per geometria e overscan, usando `switchres -g` per la regolazione.

### 6.2 Cosa non fare

- Non toccare il kernel di default né la voce di boot predefinita.
- Non promettere interlace o cambio modo sotto Hyprland finché il bug del flag non è risolto upstream.
- Non supportare HDMI né adattatori senza RTD2166/2168: documentarlo nel README e nel detect.

## 7. Lista della spesa

| Pezzo        | Scelta                                                    | Note                                                                                       |
| ------------ | --------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| DAC          | CableDeconn DP→VGA, versione non 4K senza audio (RTD2166) | Verificare il chip prima dell'acquisto; le varianti 4K non vanno.                          |
| Sync + SCART | VideoAmp con EDID, oppure UMSA + alimentazione USB        | VideoAmp toglie il problema EDID; UMSA è più reperibile in Europa (Arcadeforge, Germania). |
| Cavo SCART   | Schermato, RGB completo con pin 16 e pin 8                | Il cavo RGB-Pi attuale è specifico per il Pi e non serve qui.                              |
| Opzionale    | HDMI2SCART o RGB-Pi 2                                     | Prototipo rapido senza kernel patchato, o ripiego.                                         |

Prezzi non riportati perché non verificati da fonte diretta, salvo RGB-Pi 2 a circa 70 USD.

## 8. Rischi e incognite

1. **RDNA3 su DP con dot clock a 6-8 MHz**: confermato "in lista" dal wiki Batocera, non trovata testimonianza diretta con RX 7700/7800 XT e RTD2166. Da verificare per primo, a costo zero con il kernel patchato e `switchres -c` prima ancora di collegare il CRT.
2. **BeoCenter 1**: un thread Beoworld segnala tremolio su RGB da console anni '90 e ipotizza un'elaborazione digitale non disattivabile. Il blog dell'autore riporta però RGB-Pi a 60 Hz senza problemi sulla stessa TV, quindi il rischio è basso.
3. **Rebuild kernel**: ogni aggiornamento `linux-lts` richiede ricompilazione. Mitigazione: repository pacman con CI.
4. **DRM master e VT switch**: passare da Hyprland al VT 2 con `chvt` richiede permessi e può lasciare i monitor desktop spenti; da testare con `logind` e `seatd`.
5. **Granite Ridge**: se si volesse usare l'iGPU, il supporto non è confermato.
6. **DisplayPort 2.1 e DSC**: l'adattatore RTD2166 negozia DP 1.2; nessun problema atteso, ma il link training con clock molto bassi è il punto dove alcuni adattatori falliscono.

## 9. Piano di verifica

Ordine pensato per spendere soldi solo dopo aver eliminato le incognite software.

1. **Kernel**: costruire `linux-crt` da `linux-lts` 6.18 con le patch D0023R della cartella 6.18. Verificare che la UKI e la voce Limine compaiano da sole. Riavviare sul nuovo kernel e controllare `dmesg` per amdgpu.
2. **Switchres a secco**: `switchres 320 240 60 -c -m ntsc` e `switchres 640 480 60 -c -m ntsc` (interlacciato) per ottenere le modeline. Nessun hardware necessario.
3. **Hyprland a secco**: applicare la modeline 320x240 progressiva a una porta DP con un monitor LCD collegato. Il monitor probabilmente mostrerà "fuori range", ma `hyprctl monitors` e `dmesg` dicono se il driver ha accettato il modo. Questo verifica la parte RDNA3 senza CRT.
4. **Acquisto DAC e VideoAmp/UMSA**. Collegare al BeoCenter 1, avviare con `video=DP-x:320x240eS`, verificare immagine dalla console kernel: il primo "Omarchy" a 240p.
5. **RetroArch KMS** su VT 2 con Switchres nativo, un core SNES e un core arcade a 57.5 Hz. Misurare il refresh reale con RetroArch.
6. **480i**: GroovyMAME con un gioco interlacciato o un core PS2/Dreamcast a 480i.
7. Solo dopo: splash, menu, tema, packaging e README per gli altri utenti Omarchy.

## 10. Fonti

- Patch kernel: <https://github.com/D0023R/linux_kernel_15khz>
- Lista GPU AMD supportate: <https://github.com/ZFEbHVUE/Batocera-CRT-Script/wiki/Supported-AMD-dGPUs-&-APUs>
- DAC raccomandati: <https://github.com/ZFEbHVUE/Batocera-CRT-Script/wiki/Digital-to-Analog-(DAC)>
- Adattatori sync, SCART e transcoder: <https://github.com/ZFEbHVUE/Batocera-CRT-Script/wiki/Recommended-Adapters,-Sync-Solutions-&-Transcoders>
- Switchres: <https://github.com/antonioginer/switchres> e la discussione Wayland <https://github.com/antonioginer/switchres/issues/102>
- RetroArch CRT SwitchRes: <https://docs.libretro.com/guides/crtswitchres/> e <https://github.com/libretro/RetroArch/pull/17353>
- GroovyArcade: <https://github.com/substring/os>
- Hyprland modeline: <https://github.com/hyprwm/Hyprland/pull/2254>, bug interlace <https://github.com/hyprwm/Hyprland/issues/4607>, sorgente `src/config/shared/monitor/Parser.cpp`
- aquamarine, scarto dei modi interlacciati: `src/backend/drm/DRM.cpp`
- Omarchy, kernel e UKI: <https://github.com/omacom/omarchy/discussions/3700>
- VideoAmp: <https://www.arcade-projects.com/threads/vga-video-amplifier-board-for-arcade-monitor-with-sync-filter.24936/>
- UMSA: <https://arcadeforge.net/UMSA/UMSA-Ultimate-SCART-Adapter::57.html?language=en>
- HDMI2SCART: <https://github.com/c0pperdragon/HDMI2SCART>
- RGB-Pi 2: <https://retrorgb.com/rgb-pi-2-released.html>
- EDID override su Wayland: <https://gist.github.com/mcjmigdal/3079ca80ad6b18bf077dcadc51563fac>
- Specifiche SCART pin 16: <http://martin.hinner.info/vga/scart.html>
- BeoCenter 1 e RGB: <https://archivedforum2.beoworld.org/forums/t/35606.aspx>
