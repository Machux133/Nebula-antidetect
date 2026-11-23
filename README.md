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

Łatki nakładają się po kolei (0001 → 0008) na źródła z tagu 158.0.8127.43 (`chromium/port-patches.ps1 -Tag <wersja>` to sprawdza, procedura w `chromium/UPDATING.md`).
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
  a wynik renderowania zostaje z prawdziwego GPU. Profile w `chromium/gl_profiles.json` są **oszacowane z pamięci**, nie z prawdziwych zrzutów: złe wpisy to dokładnie ten rodzaj niespójności, który wykrywają anty-boty.
  `adapter.features`/`limits` w WebGPU nadal pokazują prawdziwy GPU. Wewnętrzna walidacja używa prawdziwych limitów.
- Szablon Mac/Linux na hoście Windows: fonty można tylko ukrywać, nie dodawać. Rasteryzacja tekstu, metryki, emoji, szerokość paska przewijania, CSS system colors,
  głosy `speechSynthesis`, kodeki i układ klawiatury zostają z Windows. `navigator.platform`, `userAgentData`, `maxTouchPoints` i ekran są podmienione, ale `matchMedia('(device-width…)')`
  i `devicePixelRatio` względem prawdziwego monitora mogą się nie zgadzać.
- Pasywny odcisk TCP/IP hosta (TTL, okno) mówi „Windows”, niezależnie od UA.

**Build.**
- To Chromium (`is_official_build=false`), nie Chrome. Brand w client hints naprawia `0006`, ale zostają: brak Widevine (wymaga binarki Google), brak kluczy Google API, brak field trials/variations jak w stabilnym Chrome.
  Wszystkie sesje podają brand „Google Chrome”; UA udające Edge byłoby niespójne (klucz `brand=` w łatce jest, ale menedżer go nie wysyła).
- Wersja 158 jest przypięta. Jest skrypt `port-patches.ps1` do sprawdzania łatek na nowym tagu, ale nie ma automatycznej aktualizacji, a skrypt nie wykryje błędów kompilacji po zmianie API.
- `--remote-debugging-port` + `--remote-allow-origins=*` zostaje otwarty na czas sesji; `--headless=new` ma własne różnice.
- Natywne łatki nie były kompilowane; nie wiemy, czy zachowują się poprawnie we wszystkich frameach i workerach.

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

## Układ repo

- `src/` menedżer (`browser.rs` startuje Chromium i buduje `--nebula-fp`, `devices.rs` szablony urządzeń, `fp.js` rozszerzenie do usunięcia, `ui/`)
- `chromium/patches/` łatki na Chromium 158.0.8127.43
- `build.ps1` pobranie źródeł, łatki, build, pakowanie do `dist/`
