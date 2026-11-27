import {
  Alert,
  Anchor,
  Autocomplete,
  Badge,
  Button,
  Card,
  Checkbox,
  Code,
  Collapse,
  Group,
  List,
  Loader,
  Progress,
  ScrollArea,
  SimpleGrid,
  Stack,
  Switch,
  TagsInput,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { ResumenResultado } from '../componentes/FrioDelModelo'
import Miniatura from '../componentes/Miniatura'
import { n, plural, tamano } from '../formato'
import { useOpciones } from '../opciones'
import { useTrabajo } from '../trabajo'
import type {
  AprobadoBandeja,
  Bandeja as BandejaT,
  DetalleGrupoBandeja,
  GrupoBandeja,
  ResultadoTransferencia,
  RevisionBandeja,
} from '../tipos'

const COLOR_CONFIANZA: Record<string, string> = { alta: 'green', media: 'yellow', baja: 'gray' }

type Que = 'escaneo' | 'aprobar' | 'extraer'

/** Lo último que se aprobó: un texto mientras corre su trabajo y otro al terminar. */
interface Hecho {
  texto: string
  listo: string
  modeloId: number | null
  sinTrabajo?: boolean
}

interface ResultadoAprobar {
  mudanza: { movidos: number; errores: string[]; carpetas_quitadas: number }
  frio: ResultadoTransferencia | null
  frio_error: string | null
}

interface ResultadoExtraer {
  carpeta: string
  entradas: number
  bytes: number
}

/** El nombre corto de un grupo: el último tramo de su carpeta (o «sueltos de …»). */
function nombreGrupo(clave: string): string {
  const sueltos = clave.endsWith('/(sueltos)')
  const k = clave.replace(/\/\(sueltos\)$/, '')
  const ultimo = k.split('/').pop() ?? k
  return sueltos ? `Sueltos de ${ultimo}` : ultimo
}

function ResumenTrabajo({ que, r }: { que: Que; r: unknown }) {
  if (!r) return null
  if (que === 'extraer') {
    const x = r as ResultadoExtraer
    return (
      <Text size="sm">
        Se extrajeron {plural(x.entradas, 'archivo', 'archivos')} ({tamano(x.bytes)}) en{' '}
        <Code>{x.carpeta}</Code>. El comprimido quedó dentro de esa carpeta.
      </Text>
    )
  }
  if (que === 'aprobar') {
    const x = r as ResultadoAprobar
    return (
      <Stack gap={4}>
        <Text size="sm">
          {plural(x.mudanza.movidos, 'archivo movido', 'archivos movidos')} a su lugar
          {x.mudanza.carpetas_quitadas > 0 && ` · ${plural(x.mudanza.carpetas_quitadas, 'carpeta vacía quitada', 'carpetas vacías quitadas')}`}
        </Text>
        {x.mudanza.errores.slice(0, 5).map((e) => (
          <Text key={e} size="xs" c="red" style={{ wordBreak: 'break-all' }}>
            {e}
          </Text>
        ))}
        {x.frio && (
          <div>
            <Text size="sm" fw={500}>
              Comprimido a frío
            </Text>
            <ResumenResultado r={x.frio} />
          </div>
        )}
        {x.frio_error && (
          <Text size="sm" c="red">
            Comprimido a frío: {x.frio_error}
          </Text>
        )}
      </Stack>
    )
  }
  const x = r as { archivos?: number; nuevos?: number; faltan?: number }
  return (
    <Text size="sm">
      Escaneo listo: {n(x.archivos ?? 0)} archivos · {n(x.nuevos ?? 0)} nuevos · {n(x.faltan ?? 0)} ya no están
    </Text>
  )
}

function TarjetaGrupo({
  g,
  destino,
  ocupado,
  onTrabajo,
  onHecho,
}: {
  g: GrupoBandeja
  destino: BandejaT['destino_frio']
  ocupado: boolean
  onTrabajo: (id: number, que: Que) => void
  onHecho: (h: Hecho) => void
}) {
  const qc = useQueryClient()
  const { filtros, etiquetas } = useOpciones()
  const p = g.propuesta
  const [f, setF] = useState({
    juego: p.juego,
    faccion: p.faccion,
    nombre: p.nombre,
    estudio: p.estudio,
    subcarpeta: p.subcarpeta,
  })
  const [et, setEt] = useState<string[]>([])
  const [aFrio, setAFrio] = useState(true)
  const [verArchivos, setVerArchivos] = useState(false)
  const [dF] = useDebouncedValue(f, 350)
  const poner = (campo: keyof typeof f) => (v: string) => setF((x) => ({ ...x, [campo]: v }))

  const rev = useQuery({
    queryKey: ['bandeja-revisar', g.clave, dF],
    queryFn: () =>
      api<RevisionBandeja>('/bandeja/revisar', { method: 'POST', body: JSON.stringify({ clave: g.clave, ...dF }) }),
    placeholderData: keepPreviousData,
  })
  const detalle = useQuery({
    queryKey: ['bandeja-grupo', g.clave],
    queryFn: () => api<DetalleGrupoBandeja>(`/bandeja/grupo?clave=${encodeURIComponent(g.clave)}`),
    enabled: verArchivos,
  })
  const refrescar = () => {
    void qc.invalidateQueries({ queryKey: ['bandeja'] })
    void qc.invalidateQueries({ queryKey: ['modelos'] })
    void qc.invalidateQueries({ queryKey: ['filtros'] })
  }
  const aprobar = useMutation({
    mutationFn: (x: { mover: boolean; juntar_con?: number }) =>
      api<AprobadoBandeja>('/bandeja/aprobar', {
        method: 'POST',
        body: JSON.stringify({
          clave: g.clave,
          ...f,
          etiquetas: et,
          mover: x.mover,
          juntar_con: x.juntar_con ?? null,
          comprimidos_a_frio: aFrio,
        }),
      }),
    onSuccess: (r, x) => {
      if (r.estado === 'conflictos') return
      const nombre = f.nombre.trim() || f.faccion.trim() || nombreGrupo(g.clave)
      if (r.trabajo_id) {
        const juntado = x.juntar_con ? juntar.find((m) => m.id === x.juntar_con)?.nombre : null
        onTrabajo(r.trabajo_id, 'aprobar')
        onHecho(
          juntado
            ? {
                texto: `Juntando los archivos de «${nombreGrupo(g.clave)}» con «${juntado}»…`,
                listo: `Los archivos de «${nombreGrupo(g.clave)}» quedaron con «${juntado}»`,
                modeloId: r.modelo_id,
              }
            : {
                texto: `«${nombre}» quedó en el catálogo; moviéndolo a su lugar…`,
                listo: `«${nombre}» quedó en el catálogo, en su lugar`,
                modeloId: r.modelo_id,
              },
        )
      } else {
        const texto =
          x.mover === false && rev.data?.se_mueve
            ? `«${nombre}» quedó en el catálogo, pendiente de mover (desde Mudanzas)`
            : `«${nombre}» quedó en el catálogo`
        onHecho({ texto, listo: texto, modeloId: r.modelo_id, sinTrabajo: true })
      }
      refrescar()
    },
  })
  const descartar = useMutation({
    mutationFn: () => api('/bandeja/descartar', { method: 'POST', body: JSON.stringify({ clave: g.clave }) }),
    onSuccess: refrescar,
  })
  const volver = useMutation({
    mutationFn: () => api('/bandeja/volver', { method: 'POST', body: JSON.stringify({ clave: g.clave }) }),
    onSuccess: refrescar,
  })
  const extraer = useMutation({
    mutationFn: (id: number) =>
      api<{ trabajo_id: number }>('/bandeja/extraer', { method: 'POST', body: JSON.stringify({ archivo_id: id }) }),
    onSuccess: (r) => onTrabajo(r.trabajo_id, 'extraer'),
  })

  const rv = rev.data
  const errores = rv?.errores ?? []
  const juntar = rv?.juntar ?? []
  const choques = juntar.length > 0 ? [] : (rv?.choques ?? [])
  const se_mueve = rv?.se_mueve ?? false
  const extraidos = g.comprimidos.filter((z) => z.extraido_en)
  const porExtraer = g.comprimidos.filter((z) => z.se_puede_extraer && !z.extraido_en)
  const bloqueado = ocupado || aprobar.isPending || rev.isFetching || !rv || errores.length > 0
  const juegos = filtros?.juegos.map((j) => j.nombre) ?? []
  const facciones = [...new Set((filtros?.facciones ?? []).filter((x) => x.juego === f.juego).map((x) => x.nombre))]
  const estudios = filtros?.estudios.map((e) => e.nombre) ?? []
  const error = aprobar.error ?? descartar.error ?? volver.error ?? extraer.error

  return (
    <Card withBorder>
      <Stack gap="sm">
        <Group align="flex-start" wrap="nowrap">
          <Miniatura
            archivoId={g.portada?.id ?? 0}
            clave={g.portada?.clave}
            lado={88}
            tipo={g.comprimidos.length && !g.mallas ? 'comprimido' : 'malla'}
          />
          <Stack gap={4} style={{ flex: 1, minWidth: 0 }}>
            <Group justify="space-between" wrap="nowrap" align="flex-start">
              <Text fw={600} style={{ wordBreak: 'break-word' }}>
                {nombreGrupo(g.clave)}
              </Text>
              <Group gap={4} wrap="nowrap" style={{ flexShrink: 0 }}>
                {g.descartado && (
                  <Badge variant="light" color="gray">
                    No mover
                  </Badge>
                )}
                {p.confianza && (
                  <Badge variant="light" color={COLOR_CONFIANZA[p.confianza]}>
                    {p.confianza}
                  </Badge>
                )}
              </Group>
            </Group>
            <Text size="xs" c="dimmed" style={{ wordBreak: 'break-all' }}>
              {g.clave}
            </Text>
            <Text size="xs" c="dimmed">
              {plural(g.archivos, 'archivo', 'archivos')} · {plural(g.mallas, 'malla', 'mallas')} · {tamano(g.bytes)}
            </Text>
            <Text size="xs">{p.porque.join(' · ')}</Text>
            <Anchor size="xs" component="button" type="button" onClick={() => setVerArchivos((v) => !v)} w="fit-content">
              {verArchivos ? 'Ocultar archivos' : 'Ver archivos'}
            </Anchor>
          </Stack>
        </Group>

        <Collapse expanded={verArchivos}>
          {detalle.isPending ? (
            <Loader size="sm" />
          ) : detalle.data ? (
            <ScrollArea.Autosize mah={220}>
              <List size="xs" spacing={2}>
                {detalle.data.archivos.map((a) => (
                  <List.Item key={a.id} style={{ wordBreak: 'break-all' }}>
                    {a.ruta.slice(g.clave.replace(/\/\(sueltos\)$/, '').length + 1) || a.nombre}{' '}
                    <Text span size="xs" c="dimmed">
                      {tamano(a.bytes)}
                    </Text>
                  </List.Item>
                ))}
              </List>
              {Object.entries(detalle.data.contenidos).map(([id, c]) => (
                <Text key={id} size="xs" c="dimmed" mt={4}>
                  {c.error
                    ? `Comprimido ${id}: ${c.error}`
                    : `Comprimido: ${plural(c.entradas ?? 0, 'archivo', 'archivos')} (${tamano(c.bytes ?? 0)}) — ${(c.nombres ?? []).slice(0, 8).join(', ')}${(c.nombres?.length ?? 0) > 8 ? '…' : ''}`}
                </Text>
              ))}
            </ScrollArea.Autosize>
          ) : null}
        </Collapse>

        {g.comprimidos.length > 0 && (
          <Stack gap={4}>
            {g.comprimidos.map((z) => (
              <Group key={z.id} justify="space-between" wrap="nowrap" gap="xs">
                <Text size="sm" style={{ wordBreak: 'break-all' }}>
                  {z.nombre}{' '}
                  <Text span size="xs" c="dimmed">
                    {tamano(z.bytes)}
                    {z.extraido_en && ' · ya extraído: viaja con el modelo'}
                  </Text>
                </Text>
                {z.se_puede_extraer && !z.extraido_en && (
                  <Button
                    size="xs"
                    variant="light"
                    color="orange"
                    disabled={ocupado}
                    loading={extraer.isPending && extraer.variables === z.id}
                    onClick={() => extraer.mutate(z.id)}
                  >
                    Extraer
                  </Button>
                )}
              </Group>
            ))}
            {porExtraer.length > 0 && (
              <Text size="xs" c="dimmed">
                Extraer comprueba el CRC de cada archivo y deja el comprimido dentro de su carpeta; al aprobar, el
                comprimido se puede mandar a frío.
              </Text>
            )}
          </Stack>
        )}

        <SimpleGrid cols={{ base: 1, sm: 2, lg: 3 }} spacing="xs" verticalSpacing="xs">
          <Autocomplete label="Juego" size="xs" data={juegos} value={f.juego} onChange={poner('juego')} />
          <Autocomplete label="Facción / tipo" size="xs" data={facciones} value={f.faccion} onChange={poner('faccion')} />
          <TextInput label="Modelo" size="xs" value={f.nombre} onChange={(e) => poner('nombre')(e.currentTarget.value)} />
          <Autocomplete label="Estudio" size="xs" data={estudios} value={f.estudio} onChange={poner('estudio')} />
          <TextInput
            label="Subcarpeta"
            size="xs"
            value={f.subcarpeta}
            onChange={(e) => poner('subcarpeta')(e.currentTarget.value)}
          />
          <TagsInput label="Etiquetas" size="xs" data={etiquetas.map((e) => e.nombre)} value={et} onChange={setEt} />
        </SimpleGrid>

        <div>
          <Text size="xs" c="dimmed">
            {!rv?.ruta_nueva ? 'Ruta nueva' : se_mueve ? 'Irá a' : 'Se queda en (ya está en su lugar)'}
          </Text>
          <Code block style={{ wordBreak: 'break-all', whiteSpace: 'pre-wrap' }}>
            {rv?.ruta_nueva || '—'}
          </Code>
        </div>
        {errores.map((e) => (
          <Text key={e} size="xs" c="red">
            {e}
          </Text>
        ))}
        {choques.length > 0 && (
          <Alert color="orange" variant="light" p="xs">
            <Text size="xs">La ruta choca con otro modelo; cambia el nombre, el estudio o la subcarpeta:</Text>
            {choques.slice(0, 3).map((c) => (
              <Text key={c} size="xs" style={{ wordBreak: 'break-all' }}>
                {c}
              </Text>
            ))}
          </Alert>
        )}
        {juntar.length > 0 && (
          <Alert color="blue" variant="light" p="xs">
            <Text size="xs">
              La ruta cae dentro de la carpeta de un modelo que ya está en su lugar: los archivos se pueden juntar con
              él (pasan a ser suyos). Usa la subcarpeta para no mezclarlos con lo que ya tiene.
            </Text>
          </Alert>
        )}
        {extraidos.length > 0 && (se_mueve || juntar.length > 0) && (
          <Checkbox
            size="xs"
            checked={aFrio && destino != null}
            disabled={destino == null}
            onChange={(e) => setAFrio(e.currentTarget.checked)}
            label={
              destino
                ? `Mandar ${extraidos.length === 1 ? 'el comprimido' : 'los comprimidos'} a frío (${destino.nombre}) y quitarlo del disco cuando el MD5 cuadre`
                : 'No hay una ubicación de frío predeterminada: el comprimido se queda en el disco'
            }
          />
        )}
        {aprobar.data?.estado === 'conflictos' && (
          <Alert color="orange" variant="light" p="xs" title="No se movió nada">
            {aprobar.data.conflictos.slice(0, 6).map((c) => (
              <Text key={c} size="xs" style={{ wordBreak: 'break-all' }}>
                {c}
              </Text>
            ))}
            {aprobar.data.conflictos.length > 6 && (
              <Text size="xs">y {n(aprobar.data.conflictos.length - 6)} más</Text>
            )}
          </Alert>
        )}
        {error && (
          <Alert color="red" variant="light" p="xs">
            {error.message}
          </Alert>
        )}

        <Group justify="space-between">
          <Group gap="xs">
            {juntar.map((m) => (
              <Button
                key={m.id}
                size="xs"
                disabled={bloqueado}
                loading={aprobar.isPending}
                onClick={() => aprobar.mutate({ mover: true, juntar_con: m.id })}
              >
                Juntar con «{m.nombre}»
              </Button>
            ))}
            {juntar.length === 0 && se_mueve && (
              <>
                <Button
                  size="xs"
                  disabled={bloqueado || choques.length > 0}
                  loading={aprobar.isPending && aprobar.variables?.mover}
                  onClick={() => aprobar.mutate({ mover: true })}
                >
                  Aprobar y mover
                </Button>
                <Button
                  size="xs"
                  variant="default"
                  disabled={bloqueado}
                  loading={aprobar.isPending && !aprobar.variables?.mover}
                  onClick={() => aprobar.mutate({ mover: false })}
                >
                  Solo agregar al catálogo
                </Button>
              </>
            )}
            {juntar.length === 0 && !se_mueve && (
              <Button size="xs" disabled={bloqueado} loading={aprobar.isPending} onClick={() => aprobar.mutate({ mover: false })}>
                Agregar al catálogo
              </Button>
            )}
          </Group>
          {g.descartado ? (
            <Button size="xs" variant="subtle" loading={volver.isPending} onClick={() => volver.mutate()}>
              Volver a proponer
            </Button>
          ) : (
            <Button
              size="xs"
              variant="subtle"
              color="gray"
              disabled={ocupado}
              loading={descartar.isPending}
              onClick={() => descartar.mutate()}
            >
              No mover
            </Button>
          )}
        </Group>
      </Stack>
    </Card>
  )
}

export default function Bandeja() {
  const qc = useQueryClient()
  const [todos, setTodos] = useState(false)
  const b = useQuery({
    queryKey: ['bandeja', todos],
    queryFn: () => api<BandejaT>(`/bandeja${todos ? '?ver=todos' : ''}`),
  })
  const [trabajo, setTrabajo] = useState<{ id: number; que: Que } | null>(null)
  const [hecho, setHecho] = useState<Hecho | null>(null)
  const t = useTrabajo(trabajo?.id)
  const escanear = useMutation({
    mutationFn: () => api<{ trabajo_id: number }>('/escaneos', { method: 'POST' }),
    onSuccess: (r) => {
      setHecho(null)
      setTrabajo({ id: r.trabajo_id, que: 'escaneo' })
    },
  })
  const estado = t.data?.estado
  useEffect(() => {
    if (estado && estado !== 'en_curso') {
      for (const k of ['bandeja', 'modelos', 'modelo', 'resumen', 'filtros', 'mudanzas', 'frio']) {
        void qc.invalidateQueries({ queryKey: [k] })
      }
    }
  }, [estado, qc])
  const ocupado = estado === 'en_curso' || escanear.isPending

  if (b.isPending) return <Loader />
  if (b.error) return <Alert color="red">{b.error.message}</Alert>
  const d = b.data
  const enEntrada = d.grupos.filter((g) => g.seccion === 'por_catalogar')
  const nuevas = d.grupos.filter((g) => g.seccion === 'nueva')
  const tarjetas = (gs: GrupoBandeja[]) => (
    <Stack gap="sm">
      {gs.map((g) => (
        <TarjetaGrupo
          key={g.clave}
          g={g}
          destino={d.destino_frio}
          ocupado={ocupado}
          onTrabajo={(id, que) => {
            if (que !== 'aprobar') setHecho(null)
            setTrabajo({ id, que })
          }}
          onHecho={(h) => {
            if (h.sinTrabajo) setTrabajo(null)
            setHecho(h)
          }}
        />
      ))}
    </Stack>
  )

  return (
    <Stack maw={1000}>
      <Group justify="space-between" align="flex-start">
        <div>
          <Title order={2}>Por catalogar</Title>
          <Text size="sm" c="dimmed">
            Lo que llegó a la colección y no es de ningún modelo. Deja las descargas en «{d.entrada}» (junto a Wargames y
            DND), escanea y apruébalas aquí: la app propone Juego, Facción y Modelo y las mueve a su lugar.
          </Text>
        </div>
        <Button variant="light" onClick={() => escanear.mutate()} loading={ocupado && trabajo?.que === 'escaneo'} disabled={ocupado}>
          Escanear
        </Button>
      </Group>

      {!d.existe_entrada && (
        <Alert color="yellow" variant="light">
          No existe la carpeta «{d.entrada}» en la colección. Créala junto a Wargames y DND y deja ahí las descargas
          nuevas.
        </Alert>
      )}
      {escanear.error && <Alert color="red">{escanear.error.message}</Alert>}
      {hecho && (
        <Alert
          color={!hecho.sinTrabajo && estado === 'fallo' ? 'red' : 'green'}
          variant="light"
          withCloseButton
          onClose={() => setHecho(null)}
        >
          {hecho.sinTrabajo || estado === 'terminado' ? hecho.listo : estado === 'fallo' ? 'No se pudo mover: mira el error abajo' : hecho.texto}
          {hecho.modeloId && (
            <>
              {' · '}
              <Anchor component={Link} to={`/catalogo/${hecho.modeloId}`} size="sm">
                ver modelo
              </Anchor>
            </>
          )}
        </Alert>
      )}
      {trabajo && t.data && (
        <Card withBorder>
          {t.data.estado === 'en_curso' ? (
            <Stack gap={4}>
              <Text size="sm">{t.data.detalle}…</Text>
              <Progress value={100} animated striped />
            </Stack>
          ) : t.data.estado === 'fallo' ? (
            <Text size="sm" c="red">
              {t.data.detalle}
            </Text>
          ) : (
            <ResumenTrabajo que={trabajo.que} r={t.data.resultado} />
          )}
        </Card>
      )}

      <Switch
        label={`Mostrar también los de «No mover» (${n(d.descartados)})`}
        checked={todos}
        onChange={(e) => setTodos(e.currentTarget.checked)}
        disabled={d.descartados === 0 && !todos}
      />

      {d.grupos.length === 0 && (
        <Text c="dimmed">No hay nada por catalogar. Todo lo que está en la colección tiene su fila en el catálogo.</Text>
      )}
      {enEntrada.length > 0 && (
        <>
          <Title order={4}>
            En «{d.entrada}» ({n(enEntrada.length)})
          </Title>
          {tarjetas(enEntrada)}
        </>
      )}
      {nuevas.length > 0 && (
        <>
          <Title order={4}>Puestos directo en su lugar ({n(nuevas.length)})</Title>
          <Text size="sm" c="dimmed">
            Carpetas de Wargames o DND que ninguna fila del catálogo cubre; los campos salen de la ruta.
          </Text>
          {tarjetas(nuevas)}
        </>
      )}
    </Stack>
  )
}
