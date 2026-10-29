# Horizon 0.2.24-elyby.2

Edición no oficial del launcher basada en `launcher@0.2.24`. Incluye cuentas
Microsoft, Ely.by y nombres offline. El juego se descarga de Horizon oficial.

## Cuenta y modo online

En **Settings → Account → Ely.by**, ingresá usuario o correo, contraseña y, si
corresponde, el código actual de segundo factor. La contraseña y el código se
borran al enviar; no se guardan. Los tokens quedan en el llavero del sistema
(Secret Service en Linux, Credential Manager en Windows). Si no está disponible,
la sesión dura sólo esta ejecución. `accounts.json` guarda nombres y UUID públicos.

La cuenta se selecciona al iniciar sesión. **Play as** cambia la cuenta activa y
**Sign out** intenta revocar el token en Ely.by y elimina la sesión local. El
launcher valida o renueva el token antes de iniciar el juego.

El juego recibe el nombre y UUID de Ely.by, además del token en un archivo de
sesión temporal. Un servicio local que sólo sirve un documento público configura
el descubrimiento de Horizon para `authserver.ely.by/session/join` y
`authserver.ely.by/session/hasJoined`. El documento no contiene credenciales y se
cierra al terminar el proceso del juego. Una instancia no puede sustituir el
proveedor elegido. Los tokens Ely.by no se envían a Microsoft.

El cliente publicado ya admite `HORIZON_API_DISCOVERY`; no se reemplaza su motor.
Su lector exige el campo histórico `user_type: msa`: es sólo el formato de
transporte de la sesión. `auth_provider: elyby` selecciona los servicios Ely.by
antes de arrancarlo; no afirma que la cuenta sea Microsoft.

El modo online requiere un servidor configurado para aceptar Ely.by. Los
servidores que verifican exclusivamente cuentas Microsoft siguen necesitando esa
cuenta. Esta edición no incorpora las skins Ely.by ni convierte una cuenta
Ely.by en una licencia de Minecraft.

## Recursos de Minecraft 26.3

Una instalación de 1.20.1 no contiene los recursos de 26.3. **Download and prepare**
puede obtener los archivos públicos desde Mojang después de validar la sesión
Ely.by. También se puede usar una instalación propia de 26.3 con **Minecraft
folder → Choose**. Las descargas comprueban tamaño y SHA-1 del manifiesto oficial;
el preparador vuelve a comprobar sus datos conocidos. El Java requerido se
instala automáticamente si hace falta. Los mundos y las instalaciones anteriores
se conservan. Los paquetes distribuidos no contienen archivos de Minecraft.

## Actualizaciones

**Install new versions automatically** viene activado de forma predeterminada:
las nuevas versiones del juego siguen el canal oficial de Horizon.

El launcher consulta GitHub Releases en `kirysoret/horizon-elyby`, con el mismo
mecanismo de distribución que el oficial. Sólo instala versiones `launcher@…-elyby.N`
firmadas por la clave pública de `elyby-update-key.txt`. Comprueba la firma
Ed25519 de `SHA256SUMS`, el tag firmado y el SHA-256 del archivo antes de reemplazar
el ejecutable. Las compilaciones originales no sustituyen esta edición. Un canal
sin publicaciones o un error de comprobación se muestra en Settings y no impide
las actualizaciones del juego. Consultar el canal e instalar la siguiente versión
requiere conexión a Internet y que el mantenedor publique una nueva compilación.

Para compilar, fijar `HORIZON_LAUNCHER_VERSION=0.2.24-elyby.2`,
`HORIZON_ELYBY_UPDATE_REPO=kirysoret/horizon-elyby` y el Client ID público original
`HORIZON_MSA_CLIENT_ID=331e08f8-172d-43b9-835a-3108c2fcdbf3`.

El workflow **Build Ely.by launcher** compila y prueba Linux y Windows, y entrega
archivos con los nombres esperados por el actualizador. El mantenedor firma y
publica esos archivos con `tools/ci/publish_elyby.sh`; la clave privada permanece
en su equipo y nunca entra al workflow. La firma protege las publicaciones, no
crea nuevas versiones por sí sola.

## Comprobaciones y límites

Pruebas de cuentas contra servicios simulados, prueba del módulo de autenticación
real de Horizon con un servicio local y pruebas del canal firmado. El formulario
se comprueba en Linux en un monitor aislado. No se usaron credenciales reales ni
se certificó una partida en un servidor remoto de Ely.by.

Windows x64 se compila con Rust y LLVM/MinGW. Destinado a Windows 10/11 x64; no se
probó la interfaz en una PC Windows real. Wine 11 no incluye `icuuc.dll`, la
biblioteca del sistema Windows que GPUI necesita para abrir la ventana. Las
pruebas de consola bajo Wine se documentan en el informe de verificación.

Protocolo: [documentación oficial de Ely.by](https://docs.ely.by/en/minecraft-auth.html).
