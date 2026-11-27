-- Los .lyt de Lychee (sobre todo los autoguardados de `<Nombre>_autosave/`) se ignoran: no son placas, no se
-- ligan como autoguardados y no llevan vista previa. Siguen en el inventario para que las mudanzas los muevan
-- con su modelo.
UPDATE archivos SET tipo = 'otro' WHERE ext = 'lyt';
DELETE FROM placa_archivos WHERE archivo_id IN (SELECT id FROM archivos WHERE ext = 'lyt');
DELETE FROM datos_placa WHERE archivo_id IN (SELECT id FROM archivos WHERE ext = 'lyt');
DELETE FROM miniaturas WHERE archivo_id IN (SELECT id FROM archivos WHERE ext = 'lyt');
UPDATE placas SET portada_archivo_id = NULL
 WHERE portada_archivo_id IN (SELECT id FROM archivos WHERE ext = 'lyt');
-- las placas detectadas que se quedaron sin archivos y sin nada del usuario (las de solo autoguardados)
DELETE FROM placas WHERE origen = 'detectada'
  AND NOT EXISTS (SELECT 1 FROM placa_archivos pa WHERE pa.placa_id = placas.id)
  AND estado = 'sin_probar' AND favorita = 0 AND notas = '' AND sin_modelo = 0
  AND portada_archivo_id IS NULL
  AND impresora_id IS NULL AND material_id IS NULL AND perfil_id IS NULL
  AND NOT EXISTS (SELECT 1 FROM placa_modelos pm WHERE pm.placa_id = placas.id AND pm.estado <> 'sugerido')
  AND NOT EXISTS (SELECT 1 FROM impresiones i WHERE i.placa_id = placas.id);
