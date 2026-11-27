# 3D Printing Archive: especificación

Versión 0.8, borrador del 26 de noviembre de 2026. La app se va a construir por fases, y este documento crece con ella. Lo que todavía no está decidido está en la sección 12.

## 1. Objetivo

Una aplicación web para manejar la colección de impresión 3D. Debe permitir saber:

- qué modelos hay, dónde están sus archivos (en el disco o en almacenamiento frío) y cómo se ven;
- qué placas y escenas existen y de qué modelos son;
- qué miniaturas impresas hay en existencia, cuánto cuestan y a quién se vendieron;
- con qué impresoras, materiales y configuraciones se imprime.

El motivo principal es el inventario para vender impresiones: hoy es difícil llevar la cuenta de las miniaturas y placas. Además, la app reemplaza la pestaña Catálogo de la hoja de Google como fuente de verdad del catálogo.

## 2. Decisiones tomadas

| Tema | Decisión |
|---|---|
| Backend | Rust: Axum (API JSON) + Tokio + SQLx sobre SQLite |
| Frontend | Aplicación aparte con React + Vite + TypeScript. En producción, el mismo binario de Rust sirve los archivos compilados |
| Dónde corre | Un mini PC en casa. El desarrollo y las pruebas se hacen en la PC actual |
| Equipo | Lenovo ThinkCentre M920q Tiny: Intel Core i5 de 8.ª o 9.ª generación con gráficos UHD 630, 16 GB de RAM y SSD NVMe de 256 GB. Admite dos discos: uno M.2 y uno SATA de 2.5" |
| Sistema | Ubuntu Server 24.04 LTS, en lugar del Windows Pro que trae |
| Archivos | La colección se muda al mini PC, a un NVMe nuevo de 2 TB que reemplaza al de 256 GB (sección 10). La PC actual la monta por red para laminar |
| Acceso | La red de casa y, desde fuera, Tailscale. Nada queda público en internet. Un solo usuario con contraseña |
| Catálogo | La app reemplaza la pestaña Catálogo. Se importan las 725 filas con las correcciones del usuario, y la hoja queda como respaldo o exportación |
| Ventas | Se llevan existencias impresas, pedidos y clientes, y costos y precio. Las licencias de venta quedan fuera por ahora (sección 13) |
| Almacenamiento frío | Google Drive (como ahora, con rclone) más un disco externo |

## 3. Lo que ya existe y se aprovecha

- **La colección** (`/mnt/documentos/Hobby/3DP`): 45,112 archivos, 326 GB. Ya está ordenada así:
  `Wargames/<Juego>/<Facción>/<Modelo> (<Estudio>)/[<Subcarpeta>]` y `DND/<Tipo>/<Modelo> (<Estudio>)`.
  - En Wargames y DND hay 35,481 mallas (321 GB), 4,819 imágenes, unas 1,150 escenas y 44 laminados.
  - Fuera del catálogo quedan Utilidades, Adornos, Yeet, Printers, Gridfinity y MTGBoxes.
- **El catálogo**: la pestaña Catálogo de la hoja «3DP: revisión de comprimidos» tiene 725 filas, todas revisadas menos una, con las columnas Ruta actual, Juego, Facción / tipo, Modelo, Estudio, Subcarpeta, Etiquetas, Confianza, Notas y Revisado.
- **El inventario** (`wargames/data/inventario.db`, SQLite). Además de los archivos tiene:
  - `inode` y `nlink`: la colección usa enlaces duros para no repetir archivos;
  - `archive_entries`: el contenido de 35,121 entradas de los comprimidos que solo están en Drive;
  - `mesh_check`: mallas revisadas;
  - `geo_hash`: huella de geometría para encontrar mallas iguales;
  - `sliced_plan`: qué laminado o escena corresponde a qué modelo.
- **Los scripts** de `wargames/scripts`: el escaneo (`inventario.py`), el catálogo y la bandeja «Por catalogar» (`catalogo_*.py`), el plan de movimientos y el ejecutor que mueve con registro y se puede deshacer por lotes (`plan_movimientos.py`, `mover_fase8.py`). La app los reemplaza, y sus reglas se pasan a Rust.
- **Almacenamiento frío actual** (Google Drive, remoto `DriveMemo:`):
  - `3DP-archivo/`: los comprimidos originales (OPR, fase 3, DakkaDakka, Pipermakes…). Muchos están **solo** ahí.
  - `3DP-cuarentena/`: laminados, OBJ originales y archivos reparados que se quitaron del disco.
- **Impresoras** (`3DP/Printers`):
  - Creality Ender-3 V3 KE (FDM, 0.4 mm), con 4 perfiles de Orca;
  - Anycubic Photon Mono y Photon Mono 4 (resina), con un perfil de resina de Lychee y archivos de calibración.
- **Renderizado sin pantalla**: f3d 3.5 ya funciona aquí y sirvió para clasificar modelos por su render.

## 4. Conceptos

| Concepto | Qué es |
|---|---|
| Biblioteca | Una raíz de archivos que la app escanea: la principal, «Por catalogar» o un disco externo |
| Archivo | Un archivo de una biblioteca: malla, escena, laminado, imagen, comprimido, documento… Un mismo contenido puede tener varios nombres (enlaces duros) |
| Modelo | Una fila del catálogo, que corresponde a una carpeta de modelo. Tiene Juego, Facción, Nombre, Estudio, Subcarpeta, Etiquetas y Notas |
| Pieza | Una malla de un modelo (cabeza, torso, arma…), con o sin soportes |
| Placa | Una placa de impresión: la escena (`.lys`, `.chitubox`, `.lyt`, `.3mf`…) y sus laminados (`.photon`, `.pm4n`, `.ctb`, `.gcode`…). Tiene una o varias piezas de uno o varios modelos, cada una con su cantidad |
| Copia | Dónde está guardado un archivo o un paquete: en el disco, en Drive o en el disco externo. Cada copia tiene su verificación (MD5 y fecha) |
| Impresión | Una vez que se imprimió una placa: fecha, impresora, material, resultado y consumo |
| Existencia | Miniaturas impresas que hay en la mano: modelo, acabado, cantidad, ubicación física y costo |
| Pedido | Lo que pidió un cliente: piezas, precios, estado y pagos |
| Colección de venta | Un conjunto armado de modelos para ofrecer (por ejemplo «Escuadra de Necrons»), con fotos, precio y descripción |
| Perfil | Los parámetros de impresión de una impresora con un material (exposición, temperatura, altura de capa…), con sus archivos del laminador |

## 5. Módulos y requisitos

### 5.1 Inventario digital
- INV-1. Escanear cada biblioteca: ruta, tamaño, fecha, inode, nlink y tipo. Para las mallas, también formato (STL binario o de texto, OBJ, 3MF), número de triángulos y caja envolvente.
- INV-2. El escaneo es incremental: solo vuelve a leer lo que cambió (tamaño, fecha o inode) y reconoce un archivo movido por su inode. Lo que ya no está se marca «falta», no se borra.
- INV-3. Mostrar los enlaces duros: «este archivo también está en …».
- INV-4. Tablero con totales por juego, facción y estudio; espacio en disco y en frío; modelos sin miniatura, sin revisar o pendientes de mover.
- INV-5. Búsqueda de texto (SQLite FTS5) por nombre, etiquetas y notas, con filtros por juego, facción, estudio, etiqueta, estado y almacenamiento.

### 5.2 Catálogo y etiquetado (reemplaza la hoja)
- CAT-1. Importar las 725 filas del Catálogo y unirlas con el inventario por la carpeta (Ruta actual).
- CAT-2. Editar los campos de un modelo y editar varios a la vez (por ejemplo, poner una etiqueta o cambiar la facción a 30 modelos). La «ruta nueva» se calcula igual que la columna J de la hoja.
- CAT-3. Etiquetas con tipo (capítulo, línea temática, «40K: …», «Proxy temático», Horus Heresy, estudio de origen…) y color. Autocompletado y sugerencias.
- CAT-4. Avisar cuando un modelo rompe las reglas acordadas:
  - nombres oficiales en inglés y el estudio entre paréntesis;
  - capítulos con ejército propio como facción;
  - los bits en `<Facción>/Bits/`;
  - los proxies temáticos como facción aparte;
  - OPR con su equivalente de 40K como etiqueta;
  - Otros, Game Aids y DND por tipo de criatura.
- CAT-5. Si la ruta nueva de un modelo no es su carpeta actual, queda «pendiente de mover». La mudanza:
  - muestra el plan antes de mover y revisa choques de nombres (también por mayúsculas);
  - mueve con enlace + quitar el nombre viejo, sin copiar y sin sobrescribir nunca;
  - se registra por lotes y se puede deshacer, como `mover_fase8.py`.
- CAT-6. Exportar el catálogo a CSV o a la hoja de Google como respaldo.
- CAT-7. Una sola marca de revisado por modelo: OK, Corregido o No mover. Se guarda quién y cuándo cambió cada campo (historial).

### 5.3 Bandeja «Por catalogar»
- INB-1. Detectar los modelos que se dejan en `Por catalogar/` con las reglas de `catalogo_modelos.py`. Se baja por las carpetas que se llaman como una facción o un estudio, y se reconocen las carpetas de piezas o de formato.
- INB-2. Proponer Juego, Facción, Modelo y Estudio a partir del nombre (facción, estudio o modelo ya conocidos), con su confianza y el porqué.
- INB-3. Revisar en la app: aceptar, corregir o mandar a «No mover». Al aprobar, mover a la ruta nueva con el mismo mecanismo de CAT-5.
- INB-4. Comprimidos en la bandeja:
  - listar su contenido, extraerlo comprobando el CRC y proponer el modelo;
  - el comprimido original va a almacenamiento frío, como se acordó para la colección.
- INB-5. Detectar los modelos que se pusieron directo en su lugar y agregarlos al catálogo leyendo la ruta.

### 5.4 Miniaturas y visor 3D
- THU-1. Una miniatura por malla (las 35 mil):
  - render sin pantalla con f3d, Z hacia arriba, cámara fija, material neutro y fondo transparente;
  - WebP de 512 px y otro de 128 px.
- THU-2. Las miniaturas se guardan en caché por contenido (inode + tamaño + fecha, o hash), así que solo se renderiza lo nuevo o lo que cambió. Se generan en una cola en segundo plano, con progreso visible.
- THU-3. Portada del modelo: la foto del estudio si existe; si no, el render de la pieza principal, o un mosaico de piezas. Se puede elegir a mano.
- THU-4. Vista lado a lado con y sin soportes cuando existan las dos versiones (por ejemplo, carpetas `Supported` y `Unsupported`).
- THU-5. Visor 3D en el navegador (three.js con react-three-fiber) bajo demanda. Las mallas muy pesadas (más de 100 MB) se muestran con una versión simplificada que genera el servidor.
- THU-6. Las placas usan la vista previa que traen dentro sus archivos: las escenas de Lychee (`.lys` y sus autoguardados `.lyt`, PNG de 512 px), los laminados de Chitubox (`.photon`, `.cbddlp`, `.ctb` sin cifrar), los de Photon Workshop (`.pws`, `.pwmo`, `.pm4n`…) y algunas escenas `.pwscene`. Se guardan en la misma caché que las miniaturas.
  - `.chitubox` y la escena de Anycubic (`.scene`) no traen imagen ni se pueden renderizar con f3d. Para ellas se usa la miniatura de la malla con el mismo nombre (la «gemela»), si existe, o una que el usuario elija entre los archivos de la placa y de sus modelos.
  - Las escenas de Lychee viejas (unas 225) tampoco traen vista previa.

### 5.5 Placas y escenas
- PLA-1. Detectar escenas (`.lys`, `.chitubox`, `.lyt`, `.scene`, `.pwscene`, `.3mf`) y laminados (`.photon`, `.pm4n`, `.pws`, `.ctb`, `.gcode`) y agruparlos en placas. Una placa reúne la escena y sus laminados para cada impresora. Reglas (fase 4):
  - una placa junta, en la misma carpeta, la escena y los laminados que se llaman igual sin la extensión (`Torso.lys` y `Torso.photon`);
  - los autoguardados de Lychee (`<Nombre>_autosave/<Nombre>_<fecha>.lyt`) van con `<Nombre>.lys` de la carpeta de arriba, aunque la escena se haya renombrado; si no hay escena, la placa se llama como la carpeta;
  - un 3MF es placa solo si es un proyecto de laminador (trae la configuración o las placas de Orca, Bambu, Prusa o Cura en `Metadata/`); un proyecto con varias placas cuenta como una, con su número de placas;
  - `.pwsr` es un perfil de resina de Photon Workshop, no una escena: va con los perfiles (fase 5);
  - la detección corre sola al final de cada escaneo y de cada mudanza, y se puede lanzar a mano. Solo vuelve a leer los archivos que cambiaron.
- PLA-2. Leer los datos de los laminados cuando el formato lo permita: impresora, altura de capa, exposición, número de capas, tiempo estimado, volumen (ml) o peso (g) y vista previa.
  - Chitubox clásico: resolución, capa, exposición (normal y de base), capas, alto, tiempo, volumen y peso.
  - Photon Workshop: nombre de la impresora, resolución, capa, exposición, capas, tiempo y volumen. El peso solo se toma si cuadra con el volumen, porque varias versiones lo guardan vacío o con basura.
  - El `.gcode` de la Ender (Orca) trae tiempo, filamento y miniatura en sus comentarios: se lee cuando haya alguno en la colección (hoy no hay).
- PLA-3. **Marcar a qué modelo pertenece cada placa.** Una placa puede tener varios modelos, cada uno con su cantidad. La app sugiere, y el usuario confirma:
  - por la carpeta donde está la placa;
  - por el nombre;
  - por la geometría (`geo_hash`, como en la fase 5).

  Hecho en la fase 4:
  - Sugerencia por la carpeta (el modelo de los archivos de la placa) y, para las placas que no están en la carpeta de un modelo, por el nombre: el nombre de modelo más largo que aparece completo en la carpeta o el nombre de la placa.
  - Se confirma placa por placa, por modelo o todas las de carpeta de una vez. Lo rechazado («no es de este») no se vuelve a sugerir.
  - **Cantidad**: miniaturas completas del modelo que salen de la placa. Vacía quiere decir que la placa solo trae partes (un brazo, la mitad de un kit). La fase 6 la usa para sumar existencias al registrar una impresión.
  - Una placa se puede marcar como «no es de ningún modelo» (calibración, pruebas).
  - Pendiente: la sugerencia por geometría. Las escenas guardan la malla en formatos propios (Lychee, Chitubox), así que hace falta leerlos primero.
- PLA-4. Desde un modelo, ver sus placas listas para imprimir y para qué impresora y material.
  - Hecho en la fase 5: la impresora se reconoce por la pantalla del laminado (su resolución, en cualquier orientación) y el perfil, por la altura de capa y la exposición. El material sale del perfil. Las placas se filtran por impresora.
- PLA-5. Crear y editar placas a mano (nombre, impresora, material, archivos, piezas y cantidades) y subir archivos de placa nuevos.
  - Hecho: separar archivos de una placa en otra nueva (hecha a mano), pasar archivos de una placa a otra y borrar las hechas a mano (sus archivos vuelven a detectarse solos). La detección no mueve lo que se puso a mano.
  - Hecho en la fase 5: elegir a mano la impresora, el perfil y el material de una placa (lo elegido manda sobre lo reconocido; la app revisa que cuadren entre sí).
  - Pendiente: subir archivos nuevos, que depende de dónde se guardan (pregunta 6 de la sección 12).
- PLA-6. Marcar placas como probadas, fallidas o favoritas, con notas.

### 5.6 Almacenamiento frío (Drive y disco externo)
- CLD-1. Ubicaciones:
  - el disco del mini PC (caliente);
  - Google Drive con los remotos de rclone configurados en el mini PC (`3DP-archivo`, `3DP-cuarentena`);
  - el disco externo, reconocido por su UUID o etiqueta.
- CLD-2. Importar lo que ya está en Drive con su MD5 (`rclone lsjson --hash`) y el contenido de los comprimidos (`archive_entries`). Así se sabe qué modelos o piezas están **solo** en frío.
- CLD-3. Estado por modelo: caliente, frío, en los dos lados o parcial, con su tamaño.
- CLD-4. Mandar un modelo a frío:
  - subir;
  - verificar el MD5 en el destino;
  - solo entonces quitar la copia local, que va primero a la papelera de la app y se borra cuando el usuario confirma.
- CLD-5. Traer un modelo de frío: bajar, extraer comprobando el CRC y volver a escanear. Funciona para modelos completos o solo para algunas piezas.
- CLD-6. El disco externo se puede consultar aunque no esté conectado (catálogo sin conexión). Cuando se conecta, se puede verificar y copiar.
- CLD-7. Las transferencias son trabajos en segundo plano, con progreso, registro y reintento.

### 5.7 Impresoras, materiales y perfiles
- PRN-1. Impresoras: nombre, tecnología (FDM o resina), marca y modelo, volumen de impresión, boquilla o pantalla, notas y documentos (manuales, mejoras, calibraciones).
- PRN-2. Materiales:
  - resinas: marca, nombre, color, presentación (por ejemplo 1 L) y precio;
  - filamentos: tipo (PLA, PETG…), marca, color, presentación (por ejemplo 1 kg), precio y densidad.
  - Opcional: cuánto queda.
- PRN-3. Perfiles por impresora y material:
  - resina: exposición normal y de base, capas de base y alzado;
  - FDM: temperaturas y velocidades;
  - en los dos, altura de capa.
  - Cada perfil lleva sus archivos del laminador (`.orca_printer`, `.lyr`…), versiones y notas como «funciona para minis de 32 mm».
- PRN-4. Importar lo que ya está en `3DP/Printers`.

Hecho en la fase 5:
- Impresoras con tecnología, marca y modelo, volumen, boquilla (FDM), pantalla y tamaño de pixel (resina), potencia (para la luz de la fase 6), carpeta de documentos y perfil normal. Su ficha lista lo que hay en su carpeta (manuales, mejoras, calibraciones). Una impresora que ya no se usa se marca así; solo se puede borrar si no tiene perfiles.
- Materiales con marca, nombre, tipo, color y muestra, presentación (ml o g), precio, densidad, diámetro (filamento) y lo que queda (opcional). La lista muestra el precio por ml o por g. No se borra un material que usan perfiles o placas.
- Perfiles por impresora y material (o «cualquier material») con altura de capa y los campos de su tecnología:
  - resina: exposición, exposición y capas de base, capas de transición, alzado, velocidades de alzado y bajada, espera con la luz apagada y antialiasing;
  - FDM: boquilla, temperaturas de boquilla y cama (y de la primera capa), ventilador, velocidad, flujo volumétrico máximo, relación de flujo, pressure advance, retracción, paredes, relleno y patrón.
  - Si cambian sus valores (material, capa o campos), la versión anterior queda guardada con su fecha y quién la cambió, y la página muestra qué cambió de una a otra. Las notas y el nombre no hacen versión.
  - Archivos del laminador ligados al perfil (se buscan por su ruta en la colección).
- Los archivos de perfiles (`.pwsr`, `.lyr`, `.orca_printer`, `.orca_filament`) ahora son del tipo «perfil», ya no escenas.
- Importación (se revisa antes de importar, no mueve archivos y no repite lo ya importado):
  - las escenas de Photon Workshop (`.pwscene`) traen la impresora (nombre, pantalla, pixel y volumen) y las resinas propias con sus valores; la activa queda como el perfil normal de la impresora. Si una escena es más vieja que otra de la misma impresora, lo que solo está en ella se ofrece sin marcar;
  - un `.pwsr` suelto se liga al perfil vigente con la misma capa y exposiciones, aunque se llame distinto; si no hay, se ofrece como perfil nuevo de la impresora de su carpeta;
  - los paquetes de Orca (`.orca_printer` y las carpetas con `bundle_structure.json`): la impresora sale del perfil de máquina, cada perfil de filamento da un material y un perfil (temperaturas, ventilador, flujo), y el perfil de proceso pone la altura de capa y el relleno. Lo que el perfil hereda de los de Orca no viene en el archivo;
  - los perfiles de Lychee (`.lyr`) vienen cifrados: se crea el perfil con el archivo ligado y sin valores, para capturarlos a mano;
  - avisos: precios que parecen en dólares o que son los de fábrica, materiales sin precio, perfiles sin temperaturas y nombres de perfil que no cuadran con la boquilla.
- La página de impresoras avisa de laminados para pantallas que no son de ninguna impresora de la app (los 44 de 1440×2560 de la Photon original, que vinieron así de los estudios).

Pendiente:
- Leer los perfiles de Chitubox y PrusaSlicer, y los `.gcode` de la Ender cuando haya.
- Llevar la cuenta de lo que queda de cada material con cada impresión (fase 6).

### 5.8 Existencias impresas
- STK-1. Una existencia es un modelo (o una pieza) con su acabado, su cantidad, su ubicación física y su costo unitario.
  - Acabado: sin pintar, con imprimación o pintada.
  - Ubicación física: por ejemplo «Caja 3».
- STK-2. Movimientos de existencia: entra por una impresión y sale por una venta, una merma o un ajuste. La cantidad siempre sale de la suma de movimientos (hay historial).
- STK-3. Registrar una impresión (placa, impresora, material, fecha, resultado y consumo) suma a las existencias las piezas de la placa según sus cantidades. Si falló, se registra la merma.
- STK-4. Vista de existencias por modelo, facción y ubicación, con alertas de existencia baja de lo que más se vende.

### 5.9 Costos y precio
- CST-1. El costo de material de una placa sale del consumo (del laminador o capturado) por el precio del material. Se reparte entre sus piezas según el volumen de cada malla o por partes iguales.
- CST-2. Costos que se pueden configurar:
  - limpieza (alcohol y guantes) por placa;
  - luz (watts × horas × precio del kWh);
  - porcentaje de fallas;
  - mano de obra por hora y pintura;
  - empaque.
- CST-3. Precio sugerido por pieza o por colección = costo × margen. Se puede ajustar a mano y se guarda el precio con que se vendió.
- CST-4. Moneda MXN por omisión (se puede configurar).

### 5.10 Pedidos y clientes
- ORD-1. Clientes: nombre, contacto (WhatsApp, teléfono, correo o red social) y notas.
- ORD-2. Pedidos con piezas, que pueden salir de existencias (se apartan) o ser por imprimir.
  - Estados: cotizado → confirmado → imprimiendo → listo → entregado, o cancelado.
  - El pago va aparte (anticipo y saldo).
- ORD-3. Las piezas por imprimir sugieren las placas que las contienen y, al imprimirse, quedan ligadas al pedido.
- ORD-4. Historial: qué se vendió, a quién, a qué precio y con qué ganancia; lo más vendido por modelo y por facción.

### 5.11 Colecciones para venta
- COL-1. Una colección de venta tiene nombre, descripción, modelos con cantidad y acabado, fotos (renders, foto del estudio o fotos propias de la miniatura pintada), precio (la suma o uno fijo) y estado (borrador, activa o archivada).
- COL-2. Exportar una colección a PDF o imagen para mandarla por WhatsApp o redes. Como la app no es pública, se comparte el archivo, no un enlace.
- COL-3. Desde una colección, ver si alcanzan las existencias o qué falta imprimir, y convertirla en pedido.

### 5.12 Cuenta y acceso
- ACC-1. Un usuario con contraseña (argon2) y sesión con cookie segura. La app escucha solo en la red de casa y en la interfaz de Tailscale.
- ACC-2. La app nunca lee ni guarda tokens de Google ni el `client_secret` de rclone. Usa los remotos que el usuario configura en el mini PC con `rclone config`.

## 6. Modelo de datos (primer borrador)

Son tablas de SQLite; las tablas de unión se escriben con `↔`.

- `bibliotecas` (id, nombre, ruta, tipo: principal | entrada | externo, activa)
- `archivos` (id, biblioteca_id, ruta, nombre, ext, tipo, bytes, mtime, inode, nlink, hash, triangulos, bbox, con_soportes, modelo_id, estado: presente | falta, visto_en)
- `modelos` (id, juego, faccion, nombre, estudio, subcarpeta, carpeta, confianza, notas, revisado, portada_archivo_id, creado, actualizado), con `historial_cambios` aparte
- `etiquetas` (id, nombre, tipo, color) ↔ `modelo_etiquetas`
- `miniaturas` (archivo_id, clave_contenido, ruta_cache, motor, estado, generado)
- `placas` (id, nombre, carpeta, clave de detección, origen: detectada | manual, estado: sin_probar | probada | fallida, favorita, sin_modelo, notas, portada elegida, malla gemela). En la fase 5 se agregan impresora_id, material_id y perfil_id.
  - ↔ `placa_archivos` (archivo_id, rol: escena | laminado | autoguardado, auto). Cada archivo es de una sola placa; `auto` = 0 si el usuario lo puso ahí.
  - ↔ `placa_modelos` (modelo_id, estado: sugerido | confirmado | rechazado, origen: carpeta | nombre | manual, cantidad).
  - `datos_placa` (archivo_id, clave, formato, es_placa, vista_previa, máquina, resolución, capa, exposición, capas, alto, tiempo, volumen, peso, número de placas, error): lo que se leyó de cada escena o laminado.
- `ubicaciones_frio` (id, nombre, tipo: drive | disco_externo, remoto o uuid, ruta base) y `copias` (archivo o paquete, ubicación, ruta, md5, verificado_en, estado). Para los comprimidos, `paquete_contenido` (las entradas de cada uno).
- `impresoras` (nombre, tecnología, marca, modelo, volumen, boquilla, pantalla, pixel, potencia, carpeta, perfil normal, en uso, notas, clave de importación), `materiales` (tipo, marca, nombre, subtipo, color, presentación, precio, densidad, diámetro, queda, en uso, notas), `perfiles` (impresora_id, material_id, capa, parametros JSON, version, en uso, notas, origen) ↔ `perfil_archivos`, y `perfil_versiones` (cómo era cada perfil antes de cambiar sus valores). Las placas guardan impresora_id, material_id y perfil_id elegidos a mano.
- `impresiones` (placa_id, impresora_id, material_id, fecha, resultado, consumo, tiempo_s, pedido_id)
- `existencias` (modelo_id, archivo_id, acabado, ubicacion_fisica, costo_unitario) y `movimientos_existencia` (existencia_id, delta, motivo, referencia, fecha)
- `clientes`, `pedidos` (cliente_id, estado, fechas, total, notas), `pedido_piezas` (modelo, existencia o colección; cantidad, acabado, precio), `pagos`
- `colecciones_venta` ↔ `coleccion_modelos` (modelo_id, cantidad, acabado, nota), con sus fotos
- `ajustes_costos` (luz, limpieza, fallas, mano de obra, margen, moneda)
- `trabajos` (tipo: escaneo | miniaturas | transferencia | mudanza; estado, progreso, registro) y `registro_archivos` (lote, acción, origen, destino, inode, resultado). Este último sirve para deshacer.

## 7. Arquitectura

```
 Navegador (PC, celular por Tailscale)
        │  HTTPS (red local / tailnet)
 ┌──────▼──────────────────────────────────────────────────────┐
 │ Mini PC (Linux x86)                                         │
 │  binario Rust (Axum)                                        │
 │   ├─ /api/v1/…  API JSON (OpenAPI con utoipa)               │
 │   ├─ /          frontend React compilado (estáticos)        │
 │   ├─ trabajadores Tokio: escaneo · miniaturas · frío ·      │
 │   │                       mudanzas  (progreso por SSE)      │
 │   └─ SQLite (WAL) + caché de miniaturas                     │
 │  colección 3DP (disco del mini PC)  ── SMB ──► PC de laminar │
 │  f3d (render) · rclone (Drive) · disco externo (USB)        │
 └─────────────────────────────────────────────────────────────┘
```

- **Backend**:
  - Axum, Tokio, SQLx (SQLite, migraciones), serde, tower-http (estáticos, compresión, trazas) y tracing;
  - argon2 y sesiones (tower-sessions);
  - walkdir para escanear;
  - lectores propios de STL, OBJ (tobj), 3MF (zip) y de las cabeceras de `.photon`, `.ctb`, `.pm4n` y `.gcode`;
  - zip, sevenz-rust, y `unrar` por línea de comandos;
  - md-5 y blake3;
  - image, para generar WebP.
- **Frontend**:
  - React + Vite + TypeScript;
  - TanStack Query (datos) y TanStack Table con virtualización (listas de miles de archivos);
  - React Router;
  - react-three-fiber + drei para el visor 3D;
  - Mantine para componentes y formularios (propuesta);
  - un cliente de TypeScript generado del OpenAPI.
- **Herramientas externas**: f3d (render) y rclone (Drive), llamadas como procesos con tiempo límite. Sus errores se guardan en el registro del trabajo.
- **Despliegue**:
  - Ubuntu Server 24.04 LTS en el mini PC, con un servicio de systemd y variables de entorno (rutas de las bibliotecas, nombres de los remotos de rclone y puerto).
  - Se instalan Tailscale, rclone (versión oficial; la de apt es vieja), f3d y Samba.
  - f3d renderiza sin pantalla con EGL sobre la gráfica Intel (Mesa). Si el paquete de apt es viejo, se usa el de su página de versiones.
  - Todo va en el NVMe de 2 TB. Ubuntu, la app, la base y la caché de miniaturas (unos 1–2 GB) van en la partición del sistema (unos 100 GB). La colección va en su propia partición ext4 con el resto (por ejemplo `/srv/3dp`).
  - Respaldo diario de la base: `sqlite3 .backup` y copia a Drive.
- **Capacidad**: el i5 de 6 núcleos y 16 GB de RAM sobran para la API y SQLite. Las 35 mil miniaturas se renderizan una sola vez en segundo plano; se estiman de 3 a 10 horas, y después solo se renderiza lo nuevo.
- **Estructura del repo**: `backend/` (crate de Rust; se separa en un workspace cuando crezca), `frontend/`, `docs/` (esta especificación y las decisiones) y `deploy/` (unidad de systemd, ejemplo de `.env`, scripts).

## 8. API (resumen)

`/api/v1`:
- `auth/login`, `auth/logout`
- `bibliotecas`, `escaneos`, `trabajos`, `trabajos/eventos` (SSE)
- `modelos`, `modelos/{id}`, `modelos/{id}/archivos`, `modelos/{id}/placas`
- `archivos/{id}`, `archivos/{id}/miniatura`, `archivos/{id}/malla`, `archivos/{id}/descarga`
- `etiquetas`, `mudanzas` (plan, ejecutar, deshacer), `bandeja`
- `placas` (lista con filtros; crear a mano), `placas/resumen`, `placas/detectar`, `placas/confirmar`, `placas/{id}` (ficha, editar, borrar), `placas/{id}/portada`, `placas/{id}/archivos`, `placas/{id}/modelos/{modelo_id}`
- `frio/ubicaciones`, `frio/transferencias`
- `impresoras`, `impresoras/{id}`, `materiales`, `materiales/{id}`, `perfiles` (filtra por impresora o material), `perfiles/campos`, `perfiles/{id}`, `perfiles/{id}/archivos`, `archivos/buscar`, `taller/resumen`, `taller/importar` (ver y ejecutar)
- `impresiones`
- `existencias`, `existencias/movimientos`
- `clientes`, `pedidos`, `pagos`
- `colecciones`, `colecciones/{id}/exportar`
- `exportar/catalogo.csv`

## 9. Reglas de seguridad de los datos

Son las mismas reglas con las que se limpió la colección:

1. Nada se borra sin verificar antes: una copia local solo se quita si existe otra verificada por MD5 o CRC.
2. Borrar quiere decir mandar a la papelera de la app. Borrar para siempre necesita confirmación explícita.
3. Toda operación que mueve, sube o quita archivos se registra por lotes y se puede deshacer o repetir. Nunca sobrescribe.
4. Los enlaces duros se respetan: mover es enlazar y quitar el nombre viejo, y quitar un nombre no pierde datos si el archivo tiene otro.
5. La carpeta Yeet y las carpetas fuera del catálogo no se tocan.
6. Los secretos (tokens de Google, `client_secret`) nunca pasan por la app ni por sus registros.

## 10. Migración desde lo actual

1. Importar `inventario.db`: archivos, mallas revisadas, `geo_hash`, `sliced_plan` y `archive_entries`.
2. Importar la pestaña Catálogo, exportada con rclone, con las 725 filas y las correcciones del usuario.
3. Listar Drive con MD5 (`3DP-archivo` y `3DP-cuarentena`) e importarlo como copias en frío.
4. Importar `3DP/Printers` como impresoras, perfiles y documentos.
5. Mudar la colección al mini PC. Hoy está en un Seagate IronWolf de 1 TB de 3.5", que no cabe en el M920q, y el SSD de 256 GB del mini PC no alcanza para 326 GB.
   - **Decidido:** el M920q no trae la charola ni el cable de la bahía de 2.5", así que se compra un **NVMe M.2 2280 de 2 TB** que reemplaza al de 256 GB.
   - La ranura M.2 del M920q es PCIe 3.0 x4: un disco PCIe 4.0 funciona, pero a velocidad de 3.0, así que no hace falta pagar de más por uno 4.0.
   - Ubuntu se instala directo en el disco nuevo, sin clonar el Windows que trae.
   - Con 2 TB quedan unos 1.5 TB libres después de la colección, con margen para traer seguido modelos del almacenamiento frío.
   - El NVMe de 256 GB que sale puede quedar en una caja USB como respaldo de la base de datos.

   La copia:
   - con `rsync -aH --info=progress2` del NTFS al ext4 del SSD nuevo (la `-H` conserva los enlaces duros);
   - comprobar archivo por archivo (tamaño y hash);
   - después, compartirla por SMB a esta PC para laminar.
6. Dejar la hoja de Google como respaldo de solo lectura, con una exportación periódica desde la app.

## 11. Fases

Cada fase termina con algo que se puede usar y se prueba primero en esta PC.

**Avance:** fases 0 a 5 terminadas (26 de noviembre de 2026). La 0 y la 1 se probaron con la colección real; las mudanzas de la 2, en colecciones de prueba (mover y deshacer dejan todo idéntico), porque en la colección real no hay nada pendiente de mover. Las miniaturas de la 3 se generaron para los 66 modelos de Necrons (999 archivos en 2 min 46 s, sin fallas, 33 MB de caché); el visor 3D falta verlo en un navegador con WebGL.

Fase 4 con la colección real:
- 1,096 placas detectadas a partir de 1,212 archivos: 1,028 escenas, 47 laminados y 137 autoguardados.
- 1,081 placas tienen sugerido su modelo por la carpeta. Las 15 sin sugerencia son proyectos 3MF fuera del catálogo (Gridfinity, MTGBoxes, Utilidades…) y las pruebas de `Printers/`.
- Las 506 vistas previas que traen los archivos salieron en 4 s. Las mallas no se renderizan para esto.
- La primera lectura tardó unos 2.5 min en el disco duro (1,272 archivos); las siguientes solo leen lo nuevo (14 s con la caché del sistema).
- Un 3MF de los Grey Knights (`GK_MKX_STRIKE_ARMS_HANDS_FORCESWORD_STYLISH_SB_repaired.3mf`) está dañado: no es un zip válido.
- Falta que el usuario confirme las sugerencias, con el botón que confirma todas las que vienen de la carpeta o placa por placa.
- No se detectan los laminados que la limpieza mandó a `3DP-cuarentena` en Drive. Se pueden ligar cuando llegue la fase 8.

Fase 5 con la colección real (probada en una copia de la base):
- La importación encontró 3 impresoras, 7 materiales y 9 perfiles en 2 escenas de Photon Workshop (más 2 de un modelo), 1 `.pwsr`, 4 paquetes de Orca y su carpeta desempacada, y 1 perfil de Lychee. Se importaron 3 impresoras, 7 materiales y 8 perfiles con 17 archivos ligados. El noveno es un «Calibrado» de la Mono 4 que solo está en escenas más viejas y no se marca solo.
- Photon Mono 4 (9024×5120): perfiles Anycubic Standard, Anycubic Water Wash y Sunlu Standard (el normal) a 0.03 mm. Photon Mono (1620×2560): «Calibrado» a 0.03 mm, 2 s, y el de Lychee. Ender-3 V3 KE: Creality Generic PLA, ML PLA Básico y PETG Barato a 0.2 mm.
- Los cuatro paquetes de Orca de la Ender se llaman «0.4» pero su boquilla es de 0.6 mm; la app lo avisa.
- Precios por revisar: la Water Wash de Anycubic ($23.99) y el ML PLA Básico ($20 por kg) parecen en dólares; el PETG Barato no trae precio.
- La Mono 4 reconoce sus 3 laminados de prueba (`Printers/SLA/Photon Mono 4`). Ninguno coincide con un perfil: son pruebas de 1.5 a 3.2 s.
- Falta que el usuario haga la importación en su base y revise los precios.

| Fase | Qué incluye | Terminada cuando |
|---|---|---|
| 0. Base | Repo, Axum + SQLite + migraciones, React + Vite, login, CI (cargo test, clippy, build del frontend), unidad de systemd, Tailscale | Se entra con contraseña desde el celular por Tailscale y se ve la página de inicio |
| 1. Inventario y catálogo (lectura) | Escaneo incremental, importación del catálogo, del inventario y del contenido de Drive, búsqueda y filtros, página de modelo con sus archivos y en qué almacenamiento están | Los 725 modelos y los 45 mil archivos se buscan y se navegan, con su estado frío o caliente |
| 2. Etiquetado y edición | Edición individual y por lote, etiquetas, reglas, historial, mudanzas con plan y deshacer, exportación CSV | La hoja de Google deja de editarse |
| 3. Miniaturas y visor | Cola de render, caché, portadas, con y sin soportes, visor 3D | Todas las mallas tienen miniatura |
| 4. Placas y escenas | Detección, lectura de laminados, vistas previas, vínculo placa ↔ modelos (sugerido y confirmado) | Cada placa existente sabe de qué modelos es |
| 5. Impresoras y materiales | Impresoras, materiales, perfiles con archivos, importación de `Printers/` | Los perfiles en uso están en la app |
| 6. Existencias y costos | Impresiones, movimientos de existencia, costo por pieza y precio sugerido | Se registra una impresión y la existencia y el costo cuadran |
| 7. Ventas | Clientes, pedidos, pagos, colecciones de venta y exportación a PDF | Se arma una colección, se manda en PDF y se registra la venta |
| 8. Almacenamiento frío | Mandar a frío y traer de frío (Drive y disco externo), verificación y papelera | Un modelo va a Drive y regresa idéntico |
| 9. Bandeja «Por catalogar» | Detección, propuesta, revisión y mudanza de descargas nuevas, comprimidos incluidos | Una descarga nueva llega a su carpeta desde la app |

La lectura del almacenamiento frío entra desde la fase 1, porque ya hay modelos que solo están en Drive. Las acciones de subir y bajar quedan para la fase 8. El orden de las fases 4 a 9 se puede cambiar según lo que haga falta primero para vender.

## 12. Preguntas abiertas

1. **Disco externo** (se ve después): ¿de qué tamaño es, con qué sistema de archivos y va conectado al mini PC o se conecta solo a veces?
2. **Formato en frío** para lo nuevo: ¿la carpeta tal cual o un comprimido por modelo? Lo que ya está en Drive son comprimidos.
3. **Existencias**: ¿se cuentan por miniatura suelta o por kit o escuadra? ¿Qué acabados manejas? ¿Tienes cajas o ubicaciones con nombre?
4. **Costos**: precio de la luz, tarifa de mano de obra o pintura, margen y porcentaje de fallas que usas hoy.
5. **Canales de venta**: WhatsApp, Facebook, Mercado Libre… Esto decide los formatos de exportación de las colecciones.
6. **Placas nuevas**: ¿dónde guardas las escenas y laminados que haces en Lychee o Chitubox? Hace falta una convención para que la app los encuentre. Hoy están junto a los modelos (las Battle Sisters, el Daemon Prince, el Judiciar…) y la app las detecta ahí; la convención decide además dónde se guardan las que se suban desde la app.
7. **Respaldo de la base**: ¿basta una copia diaria en Drive?

## 13. Fuera de alcance por ahora

- Licencias de venta por estudio. No se eligió, pero muchos modelos de Patreon solo permiten vender impresiones con un plan de comerciante. Se puede agregar después como un campo del estudio o del modelo.
- Varios usuarios y permisos, una tienda pública y pagos en línea.
- Integración con marketplaces.
- Controlar las impresoras en vivo. La Ender-3 V3 KE usa Klipper y Moonraker, así que se puede agregar después.
- Laminar desde la app.
