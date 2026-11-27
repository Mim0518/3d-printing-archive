# Instalación en el mini PC

Equipo: Lenovo ThinkCentre M920q, con Ubuntu Server 24.04 LTS en un NVMe de 2 TB (ver la especificación, secciones 2 y 10).

## 1. Sistema

1. Instala Ubuntu Server 24.04 en el NVMe nuevo, con dos particiones:
   - unos 100 GB para el sistema (`/`);
   - el resto en ext4 para la colección, montado en `/srv/3dp`.
2. Instala Tailscale y conecta el equipo a tu tailnet:
   ```bash
   curl -fsSL https://tailscale.com/install.sh | sh
   sudo tailscale up
   ```
3. Instala f3d 3.x para las miniaturas. La versión de apt es más vieja, así que usa el paquete de
   https://f3d.app (pestaña Download, .deb para Linux). Renderiza sin pantalla con EGL y necesita los
   controladores de Mesa (`sudo apt install libegl1 libgl1-mesa-dri`). Pruébalo con:
   ```bash
   f3d --output=/tmp/prueba.png --resolution=512,512 --no-background un-modelo.stl && ls -la /tmp/prueba.png
   ```
4. Crea el usuario del servicio y sus carpetas:
   ```bash
   sudo useradd --system --home /var/lib/printing-archive --shell /usr/sbin/nologin archivo
   sudo install -d -o archivo -g archivo /var/lib/printing-archive
   sudo install -d /opt/printing-archive
   ```

## 2. La app

1. Baja el paquete que arma el CI en cada push a `main`. Se compila en Ubuntu 24.04 para que coincida con el mini PC:
   ```bash
   gh run download --repo Mim0518/3d-printing-archive -n printing-archive-linux-x86_64
   tar -xzf printing-archive-*-linux-x86_64.tar.gz
   ```
   También se puede armar en el mismo mini PC con `./deploy/empaquetar.sh`, si tiene Rust y Node.
2. Instálalo:
   ```bash
   cd printing-archive-*-linux-x86_64
   sudo cp printing-archive /opt/printing-archive/
   sudo rm -rf /opt/printing-archive/dist && sudo cp -r dist /opt/printing-archive/dist
   sudo install -m 640 -o root -g archivo printing-archive.env.example /etc/printing-archive.env
   sudo cp printing-archive.service /etc/systemd/system/
   ```
3. Crea tu usuario. Pide la contraseña dos veces y no la guarda en ningún lado:
   ```bash
   sudo -u archivo env ARCHIVO_BASE=/var/lib/printing-archive/archivo.db \
     /opt/printing-archive/printing-archive usuario <tu-usuario>
   ```
   El mismo comando cambia la contraseña y cierra las sesiones abiertas.
4. Importa el catálogo y lo que está en almacenamiento frío. Los CSV salen de la hoja de Google (pestaña Catálogo) y de `wargames/scripts/exportar_frio_app.py`:
   ```bash
   cd /var/lib/printing-archive
   sudo -u archivo env $(sudo cat /etc/printing-archive.env | xargs) /opt/printing-archive/printing-archive importar catalogo hoja_catalogo.csv
   sudo -u archivo env $(sudo cat /etc/printing-archive.env | xargs) /opt/printing-archive/printing-archive importar frio frio.csv contenido.csv
   ```
   El inventario del disco se hace desde la app (Inicio → «Escanear ahora») o con el comando `escanear`.
5. Arranca el servicio:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable --now printing-archive
   journalctl -u printing-archive -f
   ```

## 3. Entrar desde el celular

```bash
sudo tailscale serve --bg 8080
```

Abre `https://<nombre-del-equipo>.<tu-tailnet>.ts.net` en cualquier dispositivo de tu tailnet. La app no queda expuesta a internet: `tailscale serve` solo atiende la tailnet. Además pone HTTPS, por eso la cookie va con `ARCHIVO_COOKIE_SEGURA=1`.

## 3.1 Almacenamiento frío (rclone)

El servicio corre como el usuario `archivo`, así que rclone necesita su propia configuración. Configúrala tú, en
la terminal del mini PC (la app nunca ve los tokens):

```bash
sudo -u archivo env RCLONE_CONFIG=/var/lib/printing-archive/rclone.conf rclone config
sudo chmod 600 /var/lib/printing-archive/rclone.conf
```

Crea ahí el remoto `DriveMemo` (Google Drive) igual que en tu PC. El `.env` ya apunta `RCLONE_CONFIG` a ese archivo.
Para probar: `sudo -u archivo env RCLONE_CONFIG=/var/lib/printing-archive/rclone.conf rclone lsd DriveMemo:`.

Un disco externo se agrega desde la app (Almacenamiento frío → Agregar una ubicación) con la carpeta donde se
monta; el usuario `archivo` debe poder leer y escribir ahí (añádela también a `ReadWritePaths` en la unidad).

La papelera de la app vive dentro de la colección, en `/srv/3dp/.archivo-papelera` (mismo disco: mover es
instantáneo y los enlaces duros se respetan). El escaneo no la ve.

## 4. Actualizar

Baja el paquete nuevo y repite el paso 2.2 (sin el `.env` si no cambió). Luego:

```bash
sudo systemctl restart printing-archive
```

Las migraciones de la base se aplican solas al arrancar.

## 5. Respaldo de la base

Mientras llega la tarea automática (sección 12, pregunta 7), basta con:

```bash
sudo -u archivo sqlite3 /var/lib/printing-archive/archivo.db ".backup '/var/lib/printing-archive/respaldo.db'"
```
