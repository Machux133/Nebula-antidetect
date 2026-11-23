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

Łatki nakładają się po kolei (0001 → 0009) na źródła z tagu 158.0.8127.43 (`chromium/port-patches.ps1 -Tag <wersja>` to sprawdza, procedura w `chromium/UPDATING.md`).
**Nie były kompilowane ani testowane w przeglądarce**: sprawdzono tylko `git apply --check`. Pierwszy build może wykazać błędy kompilacji (zwłaszcza użycie API Skia/Blink w 0002, 0005, 0007, 0008),
a nowe nagłówki `nebula_*.h` nie są w `BUILD.gn`, więc możliwy jest błąd `gn check`.

Testy: `cargo run --release --bin probe -- [przeglądarka.exe] [argumenty]` (`tests/probe`, 48 sprawdzeń: spójność w iframe i workerach, „lies”, szum, UA kontra sprzęt, automatyzacja).
Na zwykłym Edge 158 przechodzą wszystkie. **Na zbudowanym Chromium z łatkami jeszcze nie uruchamiane.** Samotest logiki szumu: `python -I chromium/tests/noise_selftest.py`.

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
9. **Odchylenia builda.** Przeglądarka podająca się za Chrome, ale zbudowana jako Chromium, ma inne brandy w client hints, brak Widevine/CDM, inne flagi i funkcje.
10. **Aktualność.** Przypięty Chromium 158, gdy użytkownicy mają już nowszy, wyróżnia się statystycznie, a część testów wersji (nowe API, zachowania) wprost to sprawdza.

## Obecne ograniczenia (stan faktyczny)

**Rozszerzenie `fp.js` (świat MAIN) zostało zredukowane, ale wciąż się ładuje.** Nawigator, ekran, strefa czasu, WebGL, canvas/audio/rects i fonty są już natywne.
W JS zostały: podmiana `AudioContext` (częstotliwość próbkowania), `enumerateDevices`, `WebSocket` (zamknięte porty) i ukrywanie `navigator.gpu`.
- To zwykłe podmiany w JS: wykrywalne punktami 1 i 2 (nie działają w workerach). Sam fakt wczytania rozszerzenia (`--load-extension`) jest wykrywalny.
- Do zrobienia: natywne odpowiedniki tych czterech rzeczy i całkowite wyłączenie rozszerzenia.

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
  głosy `speechSynthesis`, kodeki i układ klawiatury zostają z Windows. `navigator.platform`, `userAgentData`, `maxTouchPoints` i ekran są podmienione, ale `matchMedia('(device-width…)')`
  i `devicePixelRatio` względem prawdziwego monitora mogą się nie zgadzać.
- Pasywny odcisk TCP/IP hosta (TTL, okno) mówi „Windows”, niezależnie od UA, ale tylko przy połączeniu bez proxy (Direct): przez proxy strona widzi stos serwera proxy (zwykle Linux). Dlatego edycja sesji ostrzega przy szablonie Mac/Linux z Direct. Zmiana stosu na hoście Windows jest ograniczona: `DefaultTTL` w rejestrze działa na cały ruch i wymaga administratora, przełączniki `netsh` (np. timestampy TCP) nie odtworzą kolejności opcji TCP z Linuxa/macOS, a pełna zgodność wymagałaby przepisywania pakietów (WinDivert) i jest ryzykowna. Uczciwe wyjścia: profil Windows na Windowsie, Linux przez proxy z akceptacją różnic w fontach i renderowaniu, prawdziwy Linux w WSL2/VM, dla macOS prawdziwy Mac. Do sprawdzenia: p0f na własnym serwerze, czy argument o proxy się trzyma.

**Build.**
- To Chromium (`is_official_build=false`), nie Chrome. Brand w client hints naprawia `0006`, ale zostają: brak Widevine (wymaga binarki Google), brak kluczy Google API, brak field trials/variations jak w stabilnym Chrome.
  Wszystkie sesje podają brand „Google Chrome”; UA udające Edge byłoby niespójne (klucz `brand=` w łatce jest, ale menedżer go nie wysyła).
- Wersja 158 jest przypięta. Jest skrypt `port-patches.ps1` do sprawdzania łatek na nowym tagu, ale nie ma automatycznej aktualizacji, a skrypt nie wykryje błędów kompilacji po zmianie API.
- `--remote-debugging-port` + `--remote-allow-origins=*` zostaje otwarty na czas sesji; `--headless=new` ma własne różnice.
- Natywne łatki nie były kompilowane; nie wiemy, czy zachowują się poprawnie we wszystkich frameach i workerach.

**Build: pomysły do przemyślenia (nic z tego nie jest zrobione ani sprawdzone w buildzie 158).**
- *Widevine:* zamknięta binarka Google (`widevinecdm.dll` + `manifest.json`), nie ma jej w źródłach. Pomysł: skopiować katalog `WidevineCdm` z zainstalowanego Chrome (`User Data\WidevineCdm\<wersja>\_platform_specific\win_x64\`) lub Edge (host ma go w folderze Edge) do `dist/` w kroku `build.ps1`, o ile istnieje. Potrzebne: `enable_widevine = true` w `args.gn` (dziś go nie ma), potwierdzenie w źródłach 158, jak Chromium znajduje CDM (katalog obok exe czy flaga), i dopasowanie wersji CDM. Ryzyka: licencja Widevine, usługi typu Netflix mogą i tak odmówić albo obniżyć jakość, a samo „Chromium z kopiowanym CDM” też jest sygnałem. Test: `navigator.requestMediaKeySystemAccess('com.widevine.alpha', …)` i strona z testem DRM.
- *Klucze Google API:* własne klucze przez `google_api_key` w `args.gn` (projekt w Google Cloud). Bez nich wypadają tylko funkcje powiązane z Google (sync, translate). Niski priorytet.
- *Wersja 158 przypięta:* `port-patches.ps1 -Tag <nowa>` sprawdza łatki, ale nie błędy kompilacji. Każda aktualizacja to kilkugodzinny build i możliwe poprawki łatek, a Chrome stable zmienia wersję co ok. 4 tygodnie. Do decyzji: aktualizować co 2–3 wersje, nie co każdą.
- *Port debugowania:* `--remote-debugging-port` z `--remote-allow-origins=*` jest potrzebny menedżerowi (CDP: cookies, warm-up) i lokalnemu API (`debug_port`). Alternatywa: `--remote-debugging-pipe`, czyli kanał przez potok zamiast TCP. Wymaga przepisania warstwy CDP w `browser.rs` (dziś WebSocket), a `debug_port` z API zostałby tylko jako opcja na żądanie.

**Poza zakresem na dziś:** warstwa zachowania (ruch myszy, pisanie, rytm), reputacja IP i jakość proxy, TLS/HTTP2 inne niż domyślne Chromium, wycieki WebRTC poza jednym przełącznikiem.

## Co trzeba zrobić, żeby „anti-detect” miało sens

Stan: punkty 1, 2, 4 i 9 mają pierwszą wersję, punkt 3 jest częściowy (brak Widevine, wersja przypięta), punkt 2 jest niedokończony (zostają cztery podmiany w JS). Cały kod jest **niezbudowany i nieprzetestowany na prawdziwym Chromium**. Punkty 5–8 niezaczęte.
Pierwszy krok to zbudować Chromium i uruchomić `probe` oraz publiczne strony diagnostyczne; do tego czasu nie wiadomo, które z założeń działają.

Kolejność wg opłacalności:

1. **Zmierz, zamiast zakładać.** Zbuduj zestaw testowy: własne testy z punktów 1–6 oraz publiczne strony diagnostyczne (CreepJS, BrowserScan, Pixelscan, FingerprintJS demo,
   browserleaks, Sannysoft), uruchamiane automatycznie na każdej łatce i profilu. Bez tego każda zmiana to zgadywanie. Zapisuj wyniki jako regresję.
2. **Usuń `fp.js`.** Przenieś wszystko do łatek natywnych, tak aby wartości były identyczne w stronie, iframe, `about:blank` i każdym typie workera
   (m.in. `navigator.platform`, `language(s)`, `maxTouchPoints`, ekran, `userAgentData`). Natywny getter nie ma śladów punktu 1. Wyłącz ładowanie rozszerzenia.
3. **Brand i build.** Spraw, żeby `Sec-CH-UA`/`userAgentData.brands` i UA zgadzały się z deklarowaną przeglądarką, i sprawdź pozostałe różnice Chromium vs Chrome.
   Zaplanuj aktualizację wersji i skrypt portowania łatek na nową wersję (łatki rozjeżdżają się co kilka tygodni).
4. **Nie dodawaj szumu do tego, co ma być stałe.** Zamiast „±1 co 8. piksel” lepiej sprawić, by wyniki canvas/audio/rects wynikały ze spójnego, realistycznego modelu
   (te same ścieżki renderowania, brak zmian na jednolitych obszarach, spójność między API i między wątkami), a jeśli szum zostaje, to tak, by nie dało się go wykryć testami jednolitego koloru i porównaniem API.
5. **Sprawdź zgodność z populacją.** Hashe i parametry (WebGL, fonty, metryki) powinny trafiać w to, co faktycznie występuje u prawdziwych użytkowników danego urządzenia,
   a nie być unikalne na sesję. Wymaga zebrania własnych danych z prawdziwych maszyn (dziś szablony są „estimated”).
6. **Najpierw ogranicz deklaracje do tego, co host potrafi.** Najbezpieczniej: modeluj tylko własny system operacyjny i własną rodzinę GPU hosta, a inne systemy
   uruchamiaj na prawdziwym sprzęcie lub w maszynach wirtualnych z zgodnym stosem TCP/IP. Pełne udawanie Maca na Windowsie jest najtrudniejsze i najłatwiejsze do wykrycia.
7. **Warstwa sieciowa.** Zgodność strefy, języka i geolokalizacji z IP (jest), kontrola wycieków WebRTC i DNS (częściowo), jakość i typ proxy, ewentualnie zgodność TLS/HTTP2 z deklarowaną wersją Chrome.
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
- WebRTC: jest 5 trybów, ale Fake i N/A nie podmieniają adresów w kandydatach (to wymagałoby łatki natywnej), tylko blokują wycieki; Off usuwa `RTCPeerConnection` w `fp.js`. HTTP nie proxuje WebRTC.
- DNS: jest własny serwer (`1.1.1.1;8.8.8.8`, tylko zapytania A przez UDP) rozwiązywany w lokalnym przekaźniku, także dla Direct. Brak DoH i AAAA.
- Przełącznik Geo/Auto: ręczne ustawienie języków, geolokalizacji i strefy czasu (tu: tylko `geo_from_proxy` i ręczny język/strefa); geo z bazy MaxMind GeoIP.
- Szerokie formaty wpisu proxy (`ip:port@login:pass`, `login:pass@ip:port`, `ip:port:login`, `ip:port` itd.) z podpowiedzią przy polu.
- Proxy Manager: jest trwała lista w oknie PROXY (zapamiętywana), import pliku, usuwanie duplikatów i martwych, eksport. Brak: nazw, grup, protokołu per import, klonowania między pulpitami, oznaczania użytych, podpinania do presetów.

**API i automatyzacja**
- Lokalne REST API (Preferences → LOCAL API, port 0 = wyłączone, po zmianie restart): `POST /sessions/create_quick`, `POST /sessions/start {uuid, headless, debug_port}`, `POST /sessions/stop`, `GET /sessions`; tylko 127.0.0.1, żądania z nagłówkiem `Origin` odrzucane. Dodatkowo: `clone`, `delete` (do kosza), `set_proxy`, `check_proxy`, `import_cookies` (przy działającej sesji od razu, inaczej przy następnym starcie), `export_cookies` (sesja musi działać), `GET /presets` i `POST /presets/set_active` (odpowiednik „Set Active Provider”). Brak: warm-up, edycja pozostałych pól, masowe tworzenie, pulpity.
- Podpięcie Playwright/Puppeteer/Selenium przez `debug_port` zwrócony przez `start`.

**Zespół i konto**
- Wielu użytkowników w jednym koncie (do 10 subkont), role i prawa, „Only own sessions available”, dostęp do wybranych pulpitów, wspólny pulpit zespołu; Sphere Proxy współdzielone w zespole.
- Konto: licencje i plany (limity sesji, funkcje zależne od planu), płatności kryptowalutą/kartą, hasło, 2FA, usuwanie konta, powiadomienia o aktualizacjach, czat wsparcia, program poleceń, aktualizacja z poziomu aplikacji.

Import cookies odrzuca teraz duplikaty i wpisy bez nazwy/domeny.
Funkcje wymagające serwera producenta (chmura, zespół, Sphere Proxy, wysyłanie sesji, Nooklz, licencje) wykraczają poza projekt lokalny.

## Układ repo

- `src/` menedżer (`browser.rs` startuje Chromium i buduje `--nebula-fp`, `devices.rs` szablony urządzeń, `fp.js` rozszerzenie do usunięcia, `ui/`)
- `chromium/patches/` łatki na Chromium 158.0.8127.43
- `build.ps1` pobranie źródeł, łatki, build, pakowanie do `dist/`
