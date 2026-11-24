# Nebula Sessions

Menedżer sesji (Rust, eframe) + własny build Chromium 158.0.8127.43 z łatkami. Każda sesja to osobny profil
z własnym proxy i spójnym „modelem urządzenia” (rdzenie, RAM, GPU, ekran, fonty, strefa czasu…).

Build: `powershell -ExecutionPolicy Bypass -File build.ps1` (Chromium kilka godzin, ~120 GB; `-SkipChromium` buduje tylko menedżera).
Szczegóły wartości: `MECHANICS.md`.

> **Status: prototyp, nie produkt „anti-detect”.** Dziś to „fingerprint spoofer”, który oszuka proste skrypty
> (sprawdzenie `navigator.hardwareConcurrency`, hash canvasa). Nie oszuka poważnego anty-bota ani narzędzi typu CreepJS.
> Poniżej dlaczego i co z tym zrobić. Używaj tylko na kontach i stronach, do których masz prawo.

## Łatki Chromium (`chromium/patches`, kolejność ma znaczenie)

| Łatka | Co robi |
|---|---|
| `0001-nebula-device-values` | `--nebula-fp=` do wszystkich procesów; natywnie: `hardwareConcurrency`, `deviceMemory`, WebGL UNMASKED vendor/renderer, client hints (platforma, wersja, architektura, bitness) |
| `0002-nebula-noise` | natywny, deterministyczny szum: canvas (`toDataURL`/`toBlob`/`getImageData`), `OfflineAudioContext`, `Element.getBoundingClientRect/getClientRects` |
| `0003-nebula-timezone` | strefa czasu ustawiana w rendererze (ICU + V8, także workery) |
| `0004-nebula-fonts` | lista dozwolonych rodzin fontów w `FontCache` (także `local()`) |
| `0005-nebula-navigator-screen` | natywnie, także w workerach: `navigator.platform`, `language(s)`, `maxTouchPoints`, `doNotTrack`, `screen.*`, `matchMedia(device-width/height)` |
| `0006-nebula-brand` | brand „Google Chrome” w client hints; naprawa błędu z 0001 (UA z `--user-agent` i pełne client hints działają) |
| `0007-nebula-noise-v2` | zastępuje szum z 0002: canvas tylko na pikselach gradientów/krawędzi, audio ±1 ulp (także `AnalyserNode`), WebGL `readPixels`, szum szerokości glifów zamiast szumu prostokątów |
| `0008-nebula-webgl` | spójny WebGL/WebGPU z modelem GPU: lista rozszerzeń, limity, precyzja shaderów (`chromium/gl_profiles.json`) |
| `0009-nebula-build-midl` | tylko build: różnica wyniku `midl.exe` względem plików w repo (inna wersja Windows SDK) jest ostrzeżeniem, nie błędem |
| `0010`, `0011` | poprawki kompilacji z pierwszego buildu (`raw_ptr`/`memcmp` w `nebula_noise.h`, `String::FromUtf8`) |
| `0012` | błąd działania: `SkPixmap::addr8()` jest dla pikseli 1-bajtowych i przerywało renderer przy `getImageData` (teraz `addr32`) |
| `0013`, `0014` | szum canvas tylko na pikselach nieprzezroczystych i idempotentny (bit najmniej znaczący kanału zamiast ±1), żeby `getImageData` i zdekodowany `toDataURL` dawały to samo |
| `0015-nebula-touch` | zapytania CSS `pointer`/`any-pointer`/`hover`/`any-hover` zgodne z `touch=` (mysz dla 0), zamiast ekranu dotykowego hosta |
| `0016-nebula-voices` | `speechSynthesis.getVoices()` zastąpione listą modelowanej maszyny (klucz `voices=`; Windows wg języka sesji, Mac, zdalne głosy Google) |
| `0017-nebula-audio-media-websocket` | natywne zastępniki rozszerzenia: częstotliwość próbkowania `AudioContext` (`asr=`), lista urządzeń `enumerateDevices` (`mdev=`), blokada lokalnych portów WebSocket (`wsblock=`) |
| `0018-nebula-media-features-battery-connection` | zapytania CSS modelowanego ekranu i systemu: `color-gamut` (P3 dla Apple), motyw jasny i brak preferencji dostępności zamiast ustawień hosta (`gamut=`, `mqdef=`); bateria `getBattery()` laptopa albo „pełna, podłączona” dla desktopa (`batt=`); `navigator.connection` (`conn=`) |
| `0019-nebula-paste-like-human` | Ctrl+Shift+V (`PasteAndMatchStyle` z klawiatury lub menu) wpisuje schowek znak po znaku (28–120 ms, czasem dłuższa pauza, do 20 000 znaków) zamiast wklejać naraz; strona nie dostaje zdarzenia `paste`, tylko `textInput`/`input`. Odpowiednik „Paste like a human” z Linken Sphere |
| `0020-nebula-default-search` | domyślna wyszukiwarka sesji z klucza `search=google\|bing\|duckduckgo\|yandex` w `--nebula-fp` (`GetPrepopulatedFallbackSearch`); wpis w `Preferences` odpada, bo preferencja jest chroniona skrótem. Ustawienie globalne w Preferences > Browser |
| `0021-nebula-block-non-latin` | `nolatin=1`: tekst wpisany z klawiatury z literą spoza alfabetu łacińskiego (cyrylica, greka…) jest odrzucany w `EventHandler::HandleTextInputEvent`; polskie i inne litery łacińskie przechodzą. IME (chiński, japoński) idzie inną drogą i nie jest blokowane |
| `0022-nebula-geolocation` | `navigator.geolocation` podaje pozycję sesji z klucza `geo=<lat>,<lon>,<dokładność_m>` (`GeolocationProviderImpl::StartProviders`): przy włączonej pozycji prawdziwe źródła (Wi-Fi, system) nie startują wcale; pytanie o zgodę działa jak w Chrome. Pozycja to miejsce IP proxy z ip-api przesunięte o stały dla sesji odcinek (do ~3 km), dokładność 20–150 m, albo `latitude`/`longitude` z API `geo`. Bez znanej pozycji klucza nie ma i zostaje prawdziwe zachowanie |
| `0023-nebula-window-screen` | okno na modelowanym ekranie zamiast na prawdziwym monitorze: `outerWidth/outerHeight` (`ow=`, `oh=`), `screenX/screenY/screenLeft/screenTop` (`wx=`, `wy=`), `screen.availLeft/availTop`, `devicePixelRatio` i `matchMedia(resolution)` (`dpr=`, zoom strony zostaje), `screen.orientation` (`orient=landscape-primary`, kąt 0). Okno startuje zmaksymalizowane na obszarze bez paska zadań/menu (`--window-size`, `--window-position`); `innerWidth/innerHeight` zostają prawdziwe |
| `0024-nebula-quota-heap-keyboard` | `navigator.storage.estimate().quota` z modelowanego dysku 256/512/1000/2000 GB (`quota=`, wzór Chrome: 80% dysku, z tego 75% na źródło; `usage` zostaje prawdziwe), `performance.memory.jsHeapSizeLimit` zgodny z modelowanym RAM (`heap=`: 4294705152 od 8 GB w górę), `navigator.keyboard.getLayoutMap()` jako układ US QWERTY (`kbd=us`, tylko dla języka `en-*`; inne języki nadal zwracają układ hosta) |

Łatki nakładają się po kolei (0001 → 0024) na źródła z tagu 158.0.8127.43 (`chromium/port-patches.ps1 -Tag <wersja>` to sprawdza, procedura w `chromium/UPDATING.md`).
Chromium 158.0.8127.43 z łatkami 0001–0014 **zbudował się i został przetestowany** (Windows 11 x64, `build.ps1`; najpierw z `--no-sandbox`, potem z włączoną piaskownicą, patrz „Znane problemy”).

Wyniki `probe` (48 sprawdzeń) na zbudowanym Chromium z przełącznikami prawdziwej sesji (`nebula-sessions --print-args <win|mac|linux> plik`):
- preset **Windows 48/48**, **Linux 48/48**, **Mac 47/48 + 1 ostrzeżenie** (`x.fonts`: host Windows nie ma fontów Maca, więc nie da się ich potwierdzić); od łatki 0017 bez żadnego rozszerzenia;
- bez `--nebula-fp` (zwykły Chromium): 48/48. Wartości są spójne w stronie, iframe, `srcdoc`, `about:blank`, Web/Shared/Service Workerze.
- `tests/probe/extra.html`: ten sam seed daje identyczny hash canvas/audio/szerokości tekstu, inny seed inny; jednolity kolor i `div` 100 px są dokładne; strefa czasu natywna (strona i worker, z DST); brand „Google Chrome” w client hints; filtr fontów działa; `Page.navigate` z `referrer` ustawia `document.referrer`.
- Błędy znalezione dzięki testom na prawdziwym buildzie: `addr8` (0012), niespójność `getImageData` kontra `toDataURL` (0013, 0014), a przy samym budowaniu `raw_ptr`/`memcmp`/`FromUTF8` (0010, 0011).

Samotest logiki szumu: `python -I chromium/tests/noise_selftest.py`.

## Jak anty-boty wykrywają takie narzędzia

Nie szukają „łatki”. Szukają **niespójności**, czyli rzeczy, które nie mogą istnieć na jednym prawdziwym komputerze,
oraz **śladów ingerencji**. Główne mechanizmy:

1. **Ślady JS-owych podmian (tzw. „lies”).** Podmieniona funkcja/getter różni się od natywnej: `toString()` nie zwraca `[native code]`,
   zła nazwa i `length` getterów, inny kształt deskryptora właściwości, stos wywołań wskazujący na rozszerzenie, `instanceof` fałszywych obiektów
   (np. podstawiony `userAgentData`). Test: porównaj `Object.getOwnPropertyDescriptor(Navigator.prototype, 'hardwareConcurrency').get.toString()` z prawdziwą przeglądarką.
2. **Porównanie kontekstów.** Ta sama wartość pytana w stronie, w iframe, w `about:blank`, w Web Workerze, Shared Workerze i Service Workerze.
   Podmiana tylko w głównym wątku zdradza się natychmiast.
3. **Spójność między sygnałami.** UA vs `navigator.platform` vs client hints vs fonty vs GPU vs ekran vs CSS media queries vs strefa czasu vs język vs IP.
4. **Wykrywanie szumu.** Wypełnij canvas jednolitym kolorem i sprawdź, czy każdy piksel jest równy. Narysuj to samo dwa razy. Sprawdź, czy `div` o szerokości `100px`
   ma `getBoundingClientRect().width` równe dokładnie 100. Porównaj `getBoundingClientRect` z `offsetWidth`, `Range`, `ResizeObserver`. Szum zdradza się sam.
5. **Statystyka populacji.** Hash canvasa/audio/WebGL, którego nie ma w bazie prawdziwych urządzeń, jest podejrzany właśnie dlatego, że jest unikalny.
   „Unikalny na każdą sesję” to w praktyce znak rozpoznawczy tego typu narzędzi.
6. **Prawdziwy sprzęt kontra deklaracja.** Renderowanie WebGL/WebGPU, lista rozszerzeń, limity (`MAX_TEXTURE_SIZE`, precyzja shaderów), wyniki shaderów
   i metryki fontów zależą od realnego GPU, sterownika i systemu, a nie od zadeklarowanego napisu.
7. **Warstwa sieciowa.** TLS (JA3/JA4), ustawienia HTTP/2 i kolejność nagłówków, pasywny odcisk stosu TCP/IP (TTL, okno), reputacja IP, wycieki WebRTC/DNS,
   zgodność strefy czasu i języka z geolokalizacją IP.
8. **Automatyzacja i zachowanie.** `navigator.webdriver`, efekty uboczne CDP (`Runtime.enable`), otwarty port debugowania, tryb headless, nietypowe
   zdarzenia wejścia (`isTrusted`, brak ruchu myszy, idealna regularność), wzorce nawigacji.
9. **Odchylenia builda.** Przeglądarka podająca się za Chrome, ale zbudowana jako Chromium, ma inne brandy w client hints, inne flagi i funkcje (Widevine: patrz „Widevine”).
10. **Aktualność.** Przypięty Chromium 158, gdy użytkownicy mają już nowszy, wyróżnia się statystycznie, a część testów wersji (nowe API, zachowania) wprost to sprawdza.

## Obecne ograniczenia (stan faktyczny)

**Rozszerzenie `fp.js` zostało usunięte.** Wszystko, co robiło, jest natywne w łatkach, więc menedżer nie ładuje już żadnego rozszerzenia (`--load-extension`), a wartości są takie same w stronie, iframe i workerach. Konsekwencje:
- **Menedżer obsługuje wyłącznie dołączony Nebula Chromium** (folder `chromium` obok `nebula-sessions.exe`, robi go `build.ps1`). Szukanie zwykłej Chrome/Edge/Brave i ścieżka przeglądarki w Preferences zostały usunięte; bez tego folderu sesje się nie uruchamiają (komunikat w pasku i przy starcie). Przy `cargo run` skopiuj folder `chromium` z `dist/Nebula` obok `target/release/nebula-sessions.exe`.
- Przełącznik „WebGPU Hidden” został usunięty: schowanie `navigator.gpu` zostawiałoby dziesiątki globalnych `GPU*`, a prawdziwy Chrome 158 ma WebGPU. Pole `hide_webgpu` w pliku sesji jest ignorowane.
- WebRTC „Off”, „Fake” i „N/A” blokują teraz kandydatów ICE polityką `disable_non_proxied_udp` zamiast kasować `RTCPeerConnection` z `window`.

**Szum (`0007`) usuwa najprostsze „tells”, ale nie jest sprawdzony.**
- Logika pikseli/audio/glifów przeszła tylko samotest w Pythonie (`chromium/tests/noise_selftest.py`), nie kod C++ w przeglądarce.
- Jednolite obszary, przezroczystość i twarde krawędzie czytają się bez zmian; zmieniane są tylko piksele gradientów/krawędzi antyaliasingu. Cienkie linie osiowe i tekst LCD nie dostają szumu.
- Prostokąty układu nie są ruszane. Szum idzie przez szerokości glifów (≤1/128 px), co jest wiarygodne, ale niezmierzone na prawdziwych maszynach.
- Piksele z alfą <255 mogą się różnić między `getImageData` a `readPixels`. Częściowe `readPixels` nie zgadza się z `toDataURL`. `ScriptProcessor` i `AudioWorklet` bez szumu.
- Tryb „Real device” (seed z modelu urządzenia) nie daje hasha prawdziwego urządzenia, tylko hash, którego nie ma w żadnej bazie.

**Model urządzenia nie zmienia prawdziwego sprzętu.**
- WebGL (`0008`): rozszerzenia, limity i precyzja shaderów są tylko **obniżane/ukrywane** do profilu modelu. Nie da się dodać rozszerzeń ani podnieść limitów ponad prawdziwy GPU,
  a wynik renderowania zostaje z prawdziwego GPU. Profile w `chromium/gl_profiles.json`: `win-rtx3060` jest **zmierzony na prawdziwej maszynie** (RTX 3060, ANGLE D3D11), pozostałe siedem nadal **oszacowanych z pamięci** (publiczne bazy dają tylko agregaty, nie tabele per GPU). Do zmierzenia reszty: wklej `chromium/tests/capture_gl.js` w konsolę DevTools na prawdziwym sprzęcie i wstaw wynik do JSON-a. Złe wpisy: złe wpisy to dokładnie ten rodzaj niespójności, który wykrywają anty-boty.
  `adapter.features`/`limits` w WebGPU nadal pokazują prawdziwy GPU. Wewnętrzna walidacja używa prawdziwych limitów.
- Szablon Mac/Linux na hoście Windows: fonty można tylko ukrywać, nie dodawać. Rasteryzacja tekstu, metryki, emoji, szerokość paska przewijania, CSS system colors,
  głosy `speechSynthesis`, kodeki i układ klawiatury zostają z Windows. `navigator.platform`, `userAgentData`, `maxTouchPoints`, ekran, `devicePixelRatio` i geometria okna (`0005`, `0023`) są podmienione,
  ale prawdziwe okno nie może być większe od prawdziwego monitora: gdy modelowany ekran jest większy, `innerWidth/innerHeight` (prawdziwy układ) są mniejsze, niż pasowałoby do `outerWidth/outerHeight`.
- Pasywny odcisk TCP/IP hosta (TTL, okno) mówi „Windows”, niezależnie od UA, ale tylko przy połączeniu bez proxy (Direct): przez proxy strona widzi stos serwera proxy (zwykle Linux). Dlatego edycja sesji ostrzega przy szablonie Mac/Linux z Direct. Zmiana stosu na hoście Windows jest ograniczona: `DefaultTTL` w rejestrze działa na cały ruch i wymaga administratora, przełączniki `netsh` (np. timestampy TCP) nie odtworzą kolejności opcji TCP z Linuxa/macOS, a pełna zgodność wymagałaby przepisywania pakietów (WinDivert) i jest ryzykowna. Uczciwe wyjścia: profil Windows na Windowsie, Linux przez proxy z akceptacją różnic w fontach i renderowaniu, prawdziwy Linux w WSL2/VM, dla macOS prawdziwy Mac. Do sprawdzenia: p0f na własnym serwerze, czy argument o proxy się trzyma.

**Build.**
- To Chromium (`is_official_build=false`), nie Chrome. Brand w client hints naprawia `0006`, ale zostają: brak kluczy Google API, brak field trials/variations jak w stabilnym Chrome.
  Wszystkie sesje podają brand „Google Chrome”; UA „Edg/” nie jest już generowane, bo inne przeglądarki nie są obsługiwane.
- Wersja 158 jest przypięta. Jest skrypt `port-patches.ps1` do sprawdzania łatek na nowym tagu, ale nie ma automatycznej aktualizacji, a skrypt nie wykryje błędów kompilacji po zmianie API.
- Sterowanie przeglądarką idzie domyślnie przez **prywatny potok** (`--remote-debugging-pipe`): żaden port nie nasłuchuje i żaden inny proces nie może się podłączyć (zmierzone: 14 procesów przeglądarki, zero nasłuchujących portów TCP). Port debugowania otwiera się tylko, gdy lokalne API dostaje `debug_port` (zewnętrzna automatyzacja). `--headless=new` ma własne różnice.
- Natywne łatki nie były kompilowane; nie wiemy, czy zachowują się poprawnie we wszystkich frameach i workerach.

**Build: pomysły do przemyślenia (nic z tego nie jest zrobione ani sprawdzone w buildzie 158).**
- *Widevine:* zrobione, patrz sekcja „Widevine” poniżej.
- *Klucze Google API:* własne klucze przez `google_api_key` w `args.gn` (projekt w Google Cloud). Bez nich wypadają tylko funkcje powiązane z Google (sync, translate). Niski priorytet.
- *Wersja 158 przypięta:* `port-patches.ps1 -Tag <nowa>` sprawdza łatki, ale nie błędy kompilacji. Każda aktualizacja to kilkugodzinny build i możliwe poprawki łatek, a Chrome stable zmienia wersję co ok. 4 tygodnie. Do decyzji: aktualizować co 2–3 wersje, nie co każdą.
- *Port debugowania:* zrobione, patrz punkt wyżej. Połączenie CDP (`src/cdp.rs`) ma dwa transporty (potok i WebSocket); warm-up i ciasteczka używają `Target.*` na jednym połączeniu zamiast `/json/new`.

**Poza zakresem na dziś:** warstwa zachowania (ruch myszy, pisanie, rytm), reputacja IP i jakość proxy, TLS/HTTP2 inne niż domyślne Chromium (domyślne zgadza się z Chrome 158, patrz niżej), wycieki WebRTC poza jednym przełącznikiem.

## Co trzeba zrobić, żeby „anti-detect” miało sens

Stan: punkty 1, 2, 4 i 9 mają pierwszą wersję, punkt 3 jest częściowy (wersja przypięta; Widevine jest), punkt 2 jest zrobiony (rozszerzenie usunięte). Kod jest zbudowany i przetestowany na własnym Chromium (wyniki wyżej), ale **nie na stronach produkcyjnych ani narzędziach typu CreepJS/Pixelscan**. Punkty 5–8 niezaczęte.
Pierwszy krok to zbudować Chromium i uruchomić `probe` oraz publiczne strony diagnostyczne; do tego czasu nie wiadomo, które z założeń działają.

Kolejność wg opłacalności:

1. **Zmierz, zamiast zakładać.** Zbuduj zestaw testowy: własne testy z punktów 1–6 oraz publiczne strony diagnostyczne (CreepJS, BrowserScan, Pixelscan, FingerprintJS demo,
   browserleaks, Sannysoft), uruchamiane automatycznie na każdej łatce i profilu. Bez tego każda zmiana to zgadywanie. Zapisuj wyniki jako regresję.
2. **(zrobione) Usuń `fp.js`.** Przenieś wszystko do łatek natywnych, tak aby wartości były identyczne w stronie, iframe, `about:blank` i każdym typie workera
   (m.in. `navigator.platform`, `language(s)`, `maxTouchPoints`, ekran, `userAgentData`). Natywny getter nie ma śladów punktu 1. Wyłącz ładowanie rozszerzenia.
3. **Brand i build.** Spraw, żeby `Sec-CH-UA`/`userAgentData.brands` i UA zgadzały się z deklarowaną przeglądarką, i sprawdź pozostałe różnice Chromium vs Chrome.
   Zaplanuj aktualizację wersji i skrypt portowania łatek na nową wersję (łatki rozjeżdżają się co kilka tygodni).
4. **Nie dodawaj szumu do tego, co ma być stałe.** Zamiast „±1 co 8. piksel” lepiej sprawić, by wyniki canvas/audio/rects wynikały ze spójnego, realistycznego modelu
   (te same ścieżki renderowania, brak zmian na jednolitych obszarach, spójność między API i między wątkami), a jeśli szum zostaje, to tak, by nie dało się go wykryć testami jednolitego koloru i porównaniem API.
5. **Sprawdź zgodność z populacją.** Hashe i parametry (WebGL, fonty, metryki) powinny trafiać w to, co faktycznie występuje u prawdziwych użytkowników danego urządzenia,
   a nie być unikalne na sesję. Wymaga zebrania własnych danych z prawdziwych maszyn (dziś szablony są „estimated”).
6. **Najpierw ogranicz deklaracje do tego, co host potrafi.** Najbezpieczniej: modeluj tylko własny system operacyjny i własną rodzinę GPU hosta, a inne systemy
   uruchamiaj na prawdziwym sprzęcie lub w maszynach wirtualnych z zgodnym stosem TCP/IP. Pełne udawanie Maca na Windowsie jest najtrudniejsze i najłatwiejsze do wykrycia.
7. **Warstwa sieciowa.** Zgodność strefy, języka i geolokalizacji z IP (jest), kontrola wycieków WebRTC i DNS (częściowo), jakość i typ proxy, zgodność TLS/HTTP2 z Chrome (zrobione i zmierzone, patrz „Odcisk TLS”).
8. **Automatyzacja.** Jeśli sesje mają być sterowane skryptem: zamknij port debugowania dla stron, usuń ślady CDP, zadbaj o realistyczne zdarzenia wejścia i czas. To osobny, duży temat.
9. **Uczciwy opis w UI.** Dopóki punkty 1–5 nie są zrobione, nie obiecuj „niewykrywalności”. Opisuj to jako izolację profili i spójne wartości urządzenia.

## Funkcje z dokumentacji Linken Sphere (ls.app/docs), których tu nie ma

Porównanie z dokumentacją na dzień 2026-11-23 (sekcje: sesje, proxy, cookies, API, FAQ, konto). Opis wg dokumentacji producenta, niezweryfikowany w działającym programie.
Są już: pulpity, tagi, przypinanie, kosz, klonowanie, dziennik akcji, warm-up, import/eksport cookies (JSON i Netscape), „Change IP”, zbiorcze nadawanie proxy, szybkie tworzenie sesji, tryb „Real device”/„Noise”/„Off” dla canvas/audio/rects.

**Sesje**
- Presety: są gotowe OS-owe i własne („SAVE AS PRESET” w edycji sesji, wybór i usuwanie ✕ w menu presetów, używane przy tworzeniu, masowym tworzeniu i API). Preset niesie połączenie, tryby szumu, WebRTC, język/strefę, opcje przeglądarki, tagi i opis, ale nie urządzenie. Brak: tworzenia z kreatora, zakładek, rozszerzeń, AdCrusher, wyboru typu przechowywania; import XLSX/CSV ignoruje własny preset; brak presetu „Mobile”.
- Konfiguracje mobilne oraz macOS/Safari (tu: tylko szablony urządzeń z `devices.rs`).
- Ikona sesji z zestawu; opis w Markdown (kod, listy); (limity nazwy 50 i tagu 20 znaków, unikalne tagi i auto-usuwanie z kosza po 72 h są już zrobione).
- Import: jest okno „Mass creation” (pliki cookies → sesje, tabela XLSX/CSV jak w szablonie Linken Sphere, szablon CSV do pobrania; cookies wchodzą przy pierwszym starcie sesji). Brak: import haseł, kolumny Browser/Custom DNS/Webgpu/System version, protokoły ssh/freeproxy, import kont Facebook, status „New”.
- Szum także dla WebGPU i MediaDevices jako osobne przełączniki; tryb **Hybrid** (prawdziwe renderowanie + podmiana powiązanych parametrów bez szumu canvas/WebGL).
- Hasła do autouzupełniania per sesja, rozszerzenia, zakładki i strony startowe per sesja/preset.
- Synchronizacja chmurowa lub tylko lokalna, wybierana w sesji/presecie.
- Konfigurowalne kolumny listy (układ „Minimalist”/„Informative”, kolejność przeciąganiem), do sześciu „smart shortcuts” w kolumnie Actions, relatywny/bezwzględny czas.
- Filtry: system/konfiguracja, kraj proxy, status, „tylko uruchomione” (przez kogokolwiek / przeze mnie); sortowanie wg łącznego czasu pracy już jest, brak statusów (np. „New”, „Warmed” jako filtrowalnych).
- Akcje zbiorcze na zaznaczonych sesjach (eksport cookies do osobnych plików z datą, „All Actions”), Cookies Manager z podglądem cookies sesji, „Clear data” (jest „remove data”).
- Wysyłanie sesji do innego użytkownika, wysyłka do Nooklz, przenoszenie między pulpitami z okna wyboru (tu: `MoveTo`).
- Przedłużanie wygasłych cookies jest; brak wklejania cookies tekstem w oknie importu.

**Połączenie i proxy**
- Typy: Free Proxy (wbudowana pula), Sphere Proxy (rezydencyjne, płatne za ruch), Shadowsocks. Są: Direct, HTTP, SOCKS5, Tor oraz **SSH** (tunel przez systemowy klient `ssh`, hasło przez tymczasowy `SSH_ASKPASS`; nie testowane z prawdziwym serwerem) i **Localhost** (sam port, SOCKS5/HTTP wykrywany automatycznie; brak zakresów portów w presetach).
- „Live mode” dla proxy rotowanych czasowo: WebRTC, języki, geo i strefa dopasowują się do nowego IP w trakcie sesji.
- WebRTC: jest 5 trybów, ale Fake i N/A nie podmieniają adresów w kandydatach (to wymagałoby łatki natywnej), tylko blokują wycieki; Off blokuje kandydatów ICE (nie kasuje `RTCPeerConnection`). HTTP nie proxuje WebRTC.
- DNS: jest własny serwer (`1.1.1.1;8.8.8.8` przez UDP albo `https://dns.google/dns-query` jako DoH, RFC 8484 GET) rozwiązywany w lokalnym przekaźniku, także dla Direct. Najpierw rekord A, AAAA tylko gdy brak A. Nazwa serwera DoH jest rozwiązywana systemowym DNS, a wynik AAAA przez proxy HTTP nie jest ujmowany w nawiasy (brak testu na żywym serwerze).
- Dostęp do sieci lokalnej ze stron publicznych (Local Network Access): nie dodano żadnej flagi `--enable-features`. W Chromium 158 `LocalNetworkAccessChecks`, `...WebSockets` i `...WebTransport` są już domyślnie włączone (services/network/public/cpp/features.cc), więc fetch do 127.0.0.1/192.168.x.x pyta o zgodę jak w stabilnym Chrome. `LocalNetworkAccessChecksWebRTC` jest domyślnie wyłączone w Chrome, więc go nie włączamy (odcisk różniłby się od prawdziwego Chrome). Blokada portów `wsblock=` zostaje.
- Accept-Language: `--accept-lang=pl-PL,pl` Chromium zamienia sam na `pl-PL,pl;q=0.9`, a `navigator.languages` to ta sama lista; zgodne z Chrome (test `languages_match_chrome_list`).
- Przełącznik Geo/Auto: ręczne ustawienie języków, geolokalizacji i strefy czasu (tu: `geo_from_proxy`, ręczny język/strefa, pozycja `navigator.geolocation` tylko przez API `geo`); geo z bazy MaxMind GeoIP.
- Szerokie formaty wpisu proxy (`ip:port@login:pass`, `login:pass@ip:port`, `ip:port:login`, `ip:port` itd.) z podpowiedzią przy polu.
- Proxy Manager: jest trwała lista w oknie PROXY (zapamiętywana), import pliku, usuwanie duplikatów i martwych, eksport. Brak: nazw, grup, protokołu per import, klonowania między pulpitami, oznaczania użytych, podpinania do presetów.

**API i automatyzacja**
- Lokalne REST API (Preferences → LOCAL API, port 0 = wyłączone, po zmianie restart; w oryginale port ustawia się tak samo, w jego przykładach to 40080), tylko 127.0.0.1, żądania z nagłówkiem `Origin` odrzucane. Kształt wg kolekcji Postman „Linken Sphere API” (LS2 2.23.9), porównanej 2026-11-24. Test: `node tests/probe/api_test.mjs <nebula-sessions.exe> [folder chromium]` (odizolowany `APPDATA`, 59 sprawdzeń).
  - Sesje: `GET /sessions` (filtr `status`, `proxy_info`; tylko aktywny pulpit), `GET /sessions/:uuid`, `GET /sessions/actionlog` (`limit`, `offset`, `action_types`, `from_time`, `to_time`; id akcji dobrane z tekstu wpisu, więc przybliżone), `create_quick` (`count`, odpowiedź to tablica), `start` (`headless`, `debug_port`, `disable_images`, `chromium_args`; `referrer_values` jest przyjmowane i **pomijane**, odpowiedź ma wtedy `ignored`), `run_one_time` (sesje usuwane po zatrzymaniu), `stop`, `force_stop`, `delete` (także `DELETE /sessions`), `clone`, `set_name`, `set_description`, `set_misc`, `set_fingerprints`, `connection`, `set_proxy` (stare), `check_proxy` (wynik w dzienniku akcji, nie w odpowiedzi), `geo`, `set_useragent`, `import_cookies` (`file_path` albo `json`), `export_cookies` (`uuids` + `folder_path`, sesja musi działać), `start_warmup`. Metody z wieloma sesjami przyjmują `uuids` i najpierw sprawdzają wszystkie id, więc błędne żądanie niczego nie zmienia.
  - Kopie zapasowe (poza publicznym API oryginału): `POST /sessions/backup {uuid}`, `GET /sessions/backups`, `POST /sessions/restore_backup {uuid, id}`, `POST /sessions/delete_backup`.
  - Presety i pulpity: `GET/POST/DELETE /providers` (to nasze presety; `uuid` = nazwa), `GET /presets`, `POST /presets/set_active`, `GET/PUT/POST/DELETE /desktops` (nie usunie ostatniego, aktywnego ani niepustego), `POST /desktops/unlock_stopped_sessions`. `POST /app/optimize_storage` czyści tylko cache sesji (cookies i dane zostają), `POST /app/api_threads_count/:n` sprawdza zakres i nic nie robi (serwer ma wątek na połączenie).
  - Świadomie bez: `/auth/*`, `share`, `/teams` i `freeProxy` (serwer producenta). `latitude`/`longitude` w `geo` ustawiają pozycję `navigator.geolocation` (łatka 0022, zakresy -90..90 / -180..180, tylko razem). Odrzucane z komunikatem: przeglądarki inne niż `chrome` w `set_fingerprints`. `set_fingerprints` z `gpu`/`ram`/`cpu_cores` wybiera model urządzenia tego samego systemu, który ma takie wartości (lub zwraca listę dostępnych); `direct` = wartość modelu bez szumu, `fake` = szum sesji; `webgpu` jest przyjmowane i ignorowane. W `set_misc` ignorowane: `always_focus_window`, `chrome_view`, `ad_crusher`.
  - Różnica względem oryginału: `start` bez `debug_port` nie otwiera żadnego portu (`debug_port: null`), bo tak działa prywatny potok; oryginał zawsze zwraca port.
- Podpięcie Playwright/Puppeteer/Selenium przez `debug_port` przekazany w `start` (bez niego przeglądarka nie otwiera żadnego portu; odpowiedź ma wtedy `debug_port: null`).

**Zespół i konto**
- Wielu użytkowników w jednym koncie (do 10 subkont), role i prawa, „Only own sessions available”, dostęp do wybranych pulpitów, wspólny pulpit zespołu; Sphere Proxy współdzielone w zespole.
- Konto: licencje i plany (limity sesji, funkcje zależne od planu), płatności kryptowalutą/kartą, hasło, 2FA, usuwanie konta, powiadomienia o aktualizacjach, czat wsparcia, program poleceń, aktualizacja z poziomu aplikacji.

Import cookies odrzuca teraz duplikaty i wpisy bez nazwy/domeny.
Funkcje wymagające serwera producenta (chmura, zespół, Sphere Proxy, wysyłanie sesji, Nooklz, licencje) wykraczają poza projekt lokalny.

## Testy na publicznych stronach (preset Windows na hoście Windows, połączenie bezpośrednie)

| Strona | Wynik |
|---|---|
| bot.sannysoft.com | 0 błędów i 0 ostrzeżeń na 58 wierszach |
| browserscan.net/bot-detection | „Normal”: WebDriver, User-Agent, **CDP**, Navigator |
| CreepJS | wartości zgodne w workerach (GPU, UA, rdzenie, ram), brak wykrytych podmian prototypów, DOMRect/emoji/Math/JS Proxy bez uwag; `bold-fail` (hasz nieznany bazie) na **WebGL, Canvas 2d i cyfrach audio także w surowym Chromium bez łatek** (cecha builda i GPU hosta), oraz na **Timezone/Intl** z presetem (`en-US` + Europe/Warsaw jest rzadkim zestawem) |
| iphey.com, pixelscan.net | nie sprawdzone (iphey nie załadował się w czasie testu) |

Wykryte rozbieżności, których łatki jeszcze nie naprawiają (głosy syntezatora i dotyk naprawiają 0015 i 0016; listy głosów są oszacowane z pamięci, nie z prawdziwych maszyn, a głos wybrany w `speak()` i tak mówi głosem hosta):
- **Fonty:** lista modelu to ok. 56 fontów, CreepJS widzi 11/51 swoich fontów testowych (surowy Chromium na tym hoście: 17/51); „Like undefined” zostaje w każdej konfiguracji.
- **WebRTC** przy połączeniu bezpośrednim ujawnia publiczny adres IP i kandydata mDNS (ochrona działa tylko z proxy).
- WebGL/WebGPU nadal renderują prawdziwym GPU (deklarowany jest inny), `adapter.features` i `limits` są z hosta.

Stan punktu „model urządzenia”: dotyk, głosy, gamut, motyw, preferencje, bateria i `navigator.connection` pochodzą z modelu, nie z hosta (testy na modelach MacBook Air, iMac, laptop i desktop Windows). Wartości baterii i sieci są losowane z seeda sesji z prawdopodobnych zakresów (oszacowane, nie zmierzone); `dynamic-range` (HDR) i `prefers-contrast` poza „brak preferencji” nie są modelowane.

## Powierzchnia API (zmierzona względem Edge 158)

`tests/probe/apisurface.js` zbiera globalne obiekty, składowe interfejsów, właściwości CSS, `navigator` i `document`. Nasz Chromium vs Edge 158: **CSS 480 = 480, `navigator` i `document` identyczne**, globalnych 1236 vs 1241, interfejsów 959 vs 962. Tylko u nas: `LanguageModel` (API wbudowanego modelu AI) i `navigator.globalPrivacyControl`. Tylko w Edge: funkcje Privacy Sandbox (`FencedFrame`, `fence`, `document.browsingTopics`, `attributionSrc`), `XSLTProcessor`, `getDigitalGoodsService`, `hrefTranslate`. Nie zmieniam tego, bo o obecności tych rzeczy decyduje konfiguracja z serwera Google/Microsoft i bez prawdziwej Chrome nie ma wzorca; po zainstalowaniu Chrome warto porównać jeszcze raz.

## Widevine (DRM)

Widevine to zamknięty moduł Google, którego wymagają serwisy streamingowe. **Nie jest dołączany do paczki Nebula.** Chromium jest zbudowane z `enable_widevine = true` (to tylko obsługa w przeglądarce), a menedżer (`src/widevine.rs`) przy pierwszym starcie sesji kopiuje CDM z **Twojej własnej** zainstalowanej Microsoft Edge lub Google Chrome (oba trzymają go w `<wersja>\WidevineCdm`; Edge jest na Windows 10/11 prawie zawsze) do `%APPDATA%\NebulaSessions\widevine\<wersja CDM>`, a do katalogu danych każdej sesji robi dowiązania twarde (zero dodatkowego miejsca; kopia, gdy dowiązanie się nie uda). Wybiera najwyższą wersję CDM spośród znalezionych; gdy przeglądarki już nie ma, używa zapisanej kopii. Preferences > Browser pokazuje wersję, źródło i przełącznik (domyślnie włączony).

Zmierzone (CDM 4.10.3112.1 z Edge 158): `requestMediaKeySystemAccess('com.widevine.alpha')` działa (`default` i `SW_SECURE_CRYPTO`, wyższe poziomy nie, **tak samo jak w Edge**), a pełna wymiana licencji z publicznym serwerem testowym Shaka (`tests/probe/extra6.html`) kończy się pięcioma kluczami `usable`, czyli CDM deszyfruje. PlayReady jest niedostępne (jak w prawdziwej Chrome; Edge je ma).

Ograniczenia: tylko poziom L3 (programowy), więc serwisy wymagające L1 (sprzętowego, np. HD w Netflix) nie dadzą wysokiej jakości; CDM jest sprawdzany tylko na jednej wersji z Edge 158 (nie testowałem CDM z Chrome ani innych wersji); sprawdzałem licencję testową, nie konkretny serwis. Licencja: nie dystrybuujemy binarki Google, tylko kopiujemy lokalnie z przeglądarki użytkownika na jego komputerze, ale zasady korzystania z Widevine zależą od Google; to nie jest porada prawna.

## Odcisk TLS i HTTP/2 (zmierzony)

Na tym tagu Chromium (158.0.8127.43) domyślnie włączone są trzy funkcje, których stabilna Chrome 158 jeszcze nie wysyła: `TLSTrustAnchorIDs`, `NonMtcTrustAnchorIDs` (rozszerzenie TLS 0xCA34) i `AddTLSServerHandshakePadding` (4832). Przez nie JA4 był `t13d1518h2_…`, a u Chrome/Edge 158 jest `t13d1516h2_8daaf6152771_806a8c22fdea`. Menedżer przekazuje `--disable-features=TLSTrustAnchorIDs,NonMtcTrustAnchorIDs,AddTLSServerHandshakePadding`; zmierzone na tls.peet.ws w porównaniu z Edge 158 (inny program, ta sama baza Chromium): **JA4, peetprint i odcisk HTTP/2 (Akamai) są identyczne**. Nie mierzyłem z prawdziwą Google Chrome (nie ma jej na tym komputerze), więc to zgodność z Edge; TLS i HTTP/2 Chrome i Edge na jednej wersji są zwykle takie same. Po aktualizacji wersji Chromium trzeba to zmierzyć od nowa (`tests/probe` nie ma jeszcze automatycznego testu TLS).

## Czego nie zrobiono i dlaczego (potrzebne dane lub decyzja)

- **WebGPU `adapter.features` i `limits`:** host zwraca 22 funkcje i 36 limitów RTX 3060, a preset deklaruje vendor `apple` lub `intel`. Mechanizm (lista dozwolonych funkcji i górne limity, jak w WebGL) byłby prosty, ale bez prawdziwych zrzutów z Apple M-series i Intel Iris Xe zgadywanie list daje niespójność gorszą niż obecna (np. kompresja BC na Macu mogła być prawdziwa, a ASTC/ETC2 i tak nie da się dodać). Zebranie danych: otwórz `tests/probe/extra5.html` na prawdziwym urządzeniu i odczytaj `window.__r` (funkcje, limity, `info` w stronie i w workerze).
- **WebRTC „Fake IP”:** podmiana adresów kandydatów wymagałaby wpuszczenia UDP, czyli STUN z prawdziwego adresu (wyciek do serwera STUN i TURN), więc zostaje polityka blokująca. To decyzja bezpieczeństwa, nie brak łatki.
- **Fonty (zapasowe i `queryLocalFonts`):** ograniczenie zapasowych fontów do listy modelu zostawiłoby puste prostokąty tam, gdzie prawdziwy komputer modelu ma font (np. Apple Color Emoji, którego host nie ma). Wymaga prawdziwych list fontów.

## Znane problemy buildu i testów

- **Piaskownica (rozwiązane):** Chromium z własnego folderu padał z `Sandbox cannot access executable … (0x5)`, bo proces sieciowy działa w AppContainer, który musi mieć prawo odczytu dla „ALL APPLICATION PACKAGES” (i grupy LPAC) na `chrome.exe` i plikach obok. Chrome z `Program Files` ma to z instalatora, nasz folder nie, a `build.ps1` odtwarza `dist` przy każdym pakowaniu. Menedżer nadaje to prawo przy pierwszym uruchomieniu dołączonego Chromium (`browser::ensure_appcontainer_access`, `icacls`, bez administratora, znacznik `.appcontainer-acl`; dotyczy tylko folderu z plikiem `VERSION`). Jeśli uruchamiasz `chrome.exe` ręcznie z folderu buildu, wykonaj: `icacls <folder> /grant "*S-1-15-2-1:(OI)(CI)(RX)" "*S-1-15-2-2:(OI)(CI)(RX)" /T /C /Q`. Testy `probe` przechodzą z włączoną piaskownicą.
- **DO ZROBIENIA przed wydaniem: wyłączyć `DCHECK`.** Build ma `DCHECK_ALWAYS_ON=1` (domyślne dla `is_official_build=false`), więc każda wewnętrzna asercja Chromium zabija proces. Zmierzone na tym komputerze względem Edge 158: `JSON.stringify` 6–9× wolniejsze, sortowanie i pętle 1,5–3×, `chrome.dll` 449 vs 368 MB; trafiły się też dwie niepotrzebne awarie (`WorkerWatcher`, `DCHECK(false)` w piaskownicy). Głębokość rekurencji JS jest taka sama (12377 vs 12404), więc nie jest to wektor wykrycia. Kroki: `dcheck_always_on = false` w `args.gn` w `build.ps1`, pełna przebudowa (kilka godzin), powtórzyć `probe` i `extra*.html`. Warto zostawić osobny katalog `out` z `DCHECK` do rozwijania łatek (wyłapał realny błąd w 0007). Nie wiadomo, ile różnicy szybkości to `DCHECK`, a ile brak PGO/LTO (`is_official_build=false`).
- `x.fonts` dla Maca zostaje ostrzeżeniem, dopóki host nie ma fontów Maca (filtr tylko ukrywa).
- Profile WebGL w `chromium/gl_profiles.json` są oszacowane, a liczba rozszerzeń nie przekracza tego, co ma prawdziwy GPU hosta.

## Układ repo

- `src/` menedżer (`browser.rs` startuje Chromium i buduje `--nebula-fp`, `devices.rs` szablony urządzeń, `ui/`)
- `chromium/patches/` łatki na Chromium 158.0.8127.43
- `build.ps1` pobranie źródeł, łatki, build, pakowanie do `dist/`

## Braki względem Linken Sphere, dopisane po drugim przeglądzie dokumentacji (2026-11-24)

Kolejka do zrobienia, od najprostszych. `[x]` = zrobione, `[ ]` = nie.

- [x] Przełącznik „Block pop-ups” w MISC (`--disable-popup-blocking`, gdy wyłączony); skróty Ctrl+N (nowa sesja) i Ctrl+F (szukaj) w menedżerze.
- [x] Skróty na zaznaczonych sesjach: Ctrl+R start, S stop, E check proxy, H zmiana IP, B warm-up, G set proxy, D kosz, Ctrl+Shift+A zaznacz wszystko (nie działają w polu tekstowym). Brak: Ctrl+P (proxy manager), Ctrl+T, Ctrl+O, Ctrl+I i panelu pomocy „?”.
- [x] Reszta bloku Miscellaneous: Save history, Save passwords i Memory saving (menedżer zapisuje je przed startem do `Default/Preferences` i `Local State`; żadna z tych preferencji nie jest chroniona skrótem), Block non-Latin input (łatka 0021). Zmierzone w buildzie 158 (`tests/probe/misc_test.mjs`, z kontrolą, że przy włączonej historii wizyta się zapisuje). Brak: **Always focus window** (nie ma odpowiednika we flagach Chromium, wymagałoby pętli na `SetForegroundWindow`, która przeszkadza w pracy na wielu oknach).
- [x] Domyślna wyszukiwarka (Google, Bing, DuckDuckGo, Yandex; łatka 0020, `tests/probe/search_test.mjs`): Google, Bing i DuckDuckGo potwierdzone adresem nowej karty; Yandex tylko pośrednio (nowa karta to strona zewnętrzna, nie Google), bo nie ma własnej. Brak: wyboru języka interfejsu (10 języków) — celowo pominięty, wymagałby tłumaczenia całego UI.
- [x] Import zakładek (akcja „Import bookmarks” w menu sesji, tylko dla zatrzymanej sesji): plik Netscape HTML lub lista linków, foldery spłaszczone do paska zakładek, duplikaty URL pomijane, istniejący plik `Bookmarks` jest łączony, a uszkodzony nie jest nadpisywany. Zakładki widać w `chrome://bookmarks` (test w `misc_test.mjs`). Brak: importu zakładek przy tworzeniu sesji/presetu i importu haseł z CSV (per sesja).
- [x] „Paste like a human” (łatka 0019). Zmierzone na buildzie 158: tekst z polskimi znakami i emoji dochodzi w ok. 3 s, 0 zdarzeń `paste` (skrypt: `tests/probe/paste_test.mjs`). Ograniczenia: nie ma zdarzeń `keydown`/`keyup`, a „Wklej jako zwykły tekst” z menu kontekstowego też pisze powoli.
- [x] Podmiana strumienia kamery i mikrofonu (wbudowany fałszywy odbiornik Chromium, bez sterownika): w sesji plik kamery (Y4M lub MJPEG) i mikrofonu (WAV), też przez REST `set_misc` (`camera_file`, `microphone_file`). Limity: brak konwersji (MP4/MOV/JPG/PNG/GIF trzeba wcześniej zamienić na Y4M/MJPEG, np. ffmpeg); zmiana działa po ponownym starcie sesji; plik jest zapętlony, a obraz jest jeden dla wszystkich stron; etykieta urządzenia w `getUserMedia` (`track.label`) może pokazać `fake_device_0` (Chromium nadaje ją w `FakeVideoCaptureDeviceFactory`, nie jest łatana).
- [x] Wskaźnik unikalności (kropka pod polami szumu w edycji sesji i `uniqueness` w `GET /sessions/:uuid`). **Znaczenie wybrałem sam**, bo dokumentacja podaje tylko „zielony, żółty, czerwony”: zielony = żadna inna Twoja sesja nie pokazuje tej samej przeglądarki, żółty = jedna, czerwony = dwie lub więcej. „Ta sama” to ten sam model, ekran, język, strefa, user agent i ustawienia szumu (ziarno liczy się tylko przy włączonym szumie, bo tryb „Real device” daje wynik modelu, wspólny dla wszystkich sesji tego modelu). Wskaźnik **nie mierzy**, jak rzadka jest przeglądarka wśród wszystkich użytkowników: brak danych populacyjnych (punkt 5 listy wyżej). Zostaje: „Configuration pool” z prawdziwymi urządzeniami.
- [ ] Preset: pula portów Localhost, następny proxy z listy per sesja.
- [x] Kopie zapasowe sesji, lokalne (`src/backup.rs`; menu sesji: „Back up data” i „Backups…”, API: `POST /sessions/backup`, `GET /sessions/backups`, `POST /sessions/restore_backup`, `POST /sessions/delete_backup`, tylko dla zatrzymanej sesji). Kopia to pliki, które robią z profilu „zalogowany”: cookies, Local/Session Storage, IndexedDB, rejestracje service workerów, historia, hasła, `Web Data`, zakładki, ustawienia rozszerzeń i `Local State` (klucz do odszyfrowania cookies i haseł). Bez cache. Do 10 na sesję, najstarsza odpada. Przywracanie najpierw kopiuje pliki obok żywych (błąd zostawia sesję bez zmian), potem je podmienia; pliki, których kopia nie ma, są usuwane. Drugi klik „Restore” potwierdza. Wpisy w dzienniku mają id akcji 18 i 19 jak w oryginale. Brak: wyboru typów danych (oryginał ma przełączniki per typ), chmury i wymogu planu PRO. Kopie leżą w `profiles/<uuid>/backups`, więc „Remove data” ich nie rusza, a usunięcie sesji na stałe kasuje je razem z nią.
- [ ] Smart Window Control (sesje jako karty jednego okna, emulacja mobilna w oknie bez ramki).
- [ ] Inne przeglądarki (Edge/Opera/Brave, Safari) i emulacja urządzeń mobilnych: poza zakresem (jeden build Chromium).
- [ ] Porównać `src/api.rs` z pełną listą metod w Postmanie producenta (documenter.getpostman.com/view/32398185/2s9YsRd9cC), nie pobraną.
