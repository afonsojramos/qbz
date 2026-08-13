# QBZ 2.1.0 — DRAFT (notas por escribir)

> **TO-DO para el primer release con frontend Qt.** Este archivo es un
> placeholder abierto 2026-08-13 (km) para que la campaña de GPU no se
> pierda al redactar las notas. Rellenar el resto del changelog en el
> release. Referencia completa:
> `qbz-nix-docs/qt-frontend/2026-08-11-scenegraph-batches/GPU-COST-INVESTIGATION.md`

## Qt frontend — el costo de GPU del shell (95 % → 25 %)

- El shell (fondo dinámico + visualizador del NPB Large) pasó de **93-97 %
  de GPU / ~34 W a 25-26 % / ~13 W**, medido en la laptop del owner —
  mejor que la referencia Slint (35-56 %) en la misma máquina. La causa
  raíz era la tasa de presents de ventana completa, no el render: Qt Quick
  no tiene repintado parcial, y en el stack híbrido (KWin componiendo en la
  dGPU) cada present cuesta ~1.2 % de GPU fijo, sin importar el área.
- **Un solo reloj de repintado para todo el shell** (`QbzShell.pulseMs`,
  ~30 Hz, knob `QBZ_PULSE_MS`): la atmósfera del fondo, el visualizador y
  el motor de lyrics tickan en el mismo flanco. Regla permanente: ninguna
  animación continua usa `Timer`/`NumberAnimation`/`Behavior` propio, y un
  componente invisible o congelado no escribe nada.
- Fugas cerradas: el FBO del campo ambient double-presentaba; los paneles
  immersive (montados aunque el overlay esté cerrado) escribían en cada
  publish del FFT; los Behaviors de 100 ms del panel reactivo animaban a
  tasa de display (immersive: 83 % → ~25 %).
- El fondo modo 2 (blurred) compone sus cuatro capas + dim + scrim en **un
  solo ShaderEffect opaco** (paridad numérica verificada); el stack de
  imágenes queda como fallback para el renderer de software.
- **Regresa el indicador animado de now-playing en las filas de tracks**
  (eq bars en la celda de play), detrás del pref
  `play-indicator-animation` — ahora montado en el pulso, cuesta cero
  presents extra. Con el pref apagado se mantiene la pill de acento
  estática.
