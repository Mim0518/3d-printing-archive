import {
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  Checkbox,
  Group,
  Loader,
  Modal,
  Progress,
  ScrollArea,
  Select,
  Stack,
  Table,
  Text,
} from '@mantine/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { cuando, n, plural, tamano } from '../formato'
import { ESTADO_FRIO, useRefrescarFrio } from '../opciones'
import { useTrabajo } from '../trabajo'
import type { EstadoFrioModelo, PlanSubir, ResultadoTransferencia } from '../tipos'

/** Lo que salió de un trabajo de frío, en una o dos líneas. */
export function ResumenResultado({ r }: { r: ResultadoTransferencia | null | undefined }) {
  if (!r) return null
  const partes = [
    r.subidos != null ? `${plural(r.subidos, 'subido', 'subidos')}` : '',
    r.ya_estaban ? `${n(r.ya_estaban)} ya estaban` : '',
    r.verificados != null ? `${n(r.verificados)} verificados por MD5` : '',
    r.traidos != null ? `${plural(r.traidos, 'traído', 'traídos')}` : '',
    r.extraidos ? `${n(r.extraidos)} extraídos de comprimidos` : '',
    r.a_papelera ? `${n(r.a_papelera)} a la papelera` : '',
    r.sin_copia ? `${n(r.sin_copia)} sin copia igual (se quedan)` : '',
  ].filter(Boolean)
  const problemas = [...(r.errores ?? []), ...(r.conflictos ?? []), ...(r.no_quitados ?? [])]
  return (
    <Stack gap={2}>
      {partes.length > 0 && <Text size="sm">{partes.join(' · ')}</Text>}
      {r.error && (
        <Text size="sm" c="red">
          {r.error}
        </Text>
      )}
      {problemas.slice(0, 5).map((p) => (
        <Text key={p} size="xs" c="orange" style={{ wordBreak: 'break-all' }}>
          {p}
        </Text>
      ))}
      {problemas.length > 5 && (
        <Text size="xs" c="orange">
          y {n(problemas.length - 5)} más
        </Text>
      )}
      {r.apartado_en && (
        <Text size="xs" c="dimmed" style={{ wordBreak: 'break-all' }}>
          Lo que no se pudo poner en su lugar quedó en {r.apartado_en}
        </Text>
      )}
      {(r.avisos ?? []).map((a) => (
        <Text key={a} size="xs" c="dimmed">
          {a}
        </Text>
      ))}
    </Stack>
  )
}

function Mandar({ modeloId, e, onTrabajo }: { modeloId: number; e: EstadoFrioModelo; onTrabajo: (id: number) => void }) {
  const [ubicacion, setUbicacion] = useState<string | null>(
    String(e.destinos.find((d) => d.predeterminada)?.id ?? e.destinos[0]?.id ?? ''),
  )
  const [quitar, setQuitar] = useState(false)
  const plan = useQuery({
    queryKey: ['frio-plan', modeloId, ubicacion],
    queryFn: () => api<PlanSubir>(`/frio/modelos/${modeloId}/plan?ubicacion_id=${ubicacion}`),
    enabled: !!ubicacion,
  })
  const subir = useMutation({
    mutationFn: () =>
      api<{ trabajo_id: number }>(`/frio/modelos/${modeloId}/subir`, {
        method: 'POST',
        body: JSON.stringify({ ubicacion_id: Number(ubicacion), quitar_local: quitar }),
      }),
    onSuccess: (r) => onTrabajo(r.trabajo_id),
  })
  const p = plan.data
  return (
    <Stack gap="sm">
      <Select
        label="A dónde"
        data={e.destinos.map((d) => ({ value: String(d.id), label: d.nombre }))}
        value={ubicacion}
        allowDeselect={false}
        onChange={setUbicacion}
      />
      {plan.isPending && <Loader size="sm" />}
      {p && (
        <>
          <Text size="sm">
            Se copian {plural(p.archivos, 'archivo', 'archivos')} ({tamano(p.bytes)}) a{' '}
            <Text span ff="monospace" size="xs" style={{ wordBreak: 'break-all' }}>
              {p.destino}
            </Text>
            , cada uno con su misma ruta. Después se lista el destino y se compara el MD5 de cada archivo.
          </Text>
          {p.ya_en_frio > 0 && (
            <Text size="sm" c="dimmed">
              {n(p.ya_en_frio)} ya tienen copia verificada: no se vuelven a subir si no cambiaron.
            </Text>
          )}
          {p.avisos.map((a) => (
            <Text key={a} size="sm" c="orange">
              {a}
            </Text>
          ))}
          <Checkbox
            checked={quitar}
            onChange={(ev) => setQuitar(ev.currentTarget.checked)}
            label="Y quitarlo del disco si todo se verificó"
            description={
              <>
                Los archivos van a la papelera de la app (no se borran): se pueden restaurar, y solo se borran para siempre
                cuando la vacíes.
                {p.compartidos > 0 &&
                  ` ${plural(p.compartidos, 'archivo comparte', 'archivos comparten')} contenido con otro modelo (enlace duro): esos ${tamano(p.bytes_compartidos)} no se liberan.`}
              </>
            }
          />
        </>
      )}
      {subir.error && <Alert color="red">{subir.error.message}</Alert>}
      <Button disabled={!p || p.archivos === 0} loading={subir.isPending} onClick={() => subir.mutate()}>
        {quitar ? 'Mandar a frío y quitar del disco' : 'Mandar a frío'}
      </Button>
    </Stack>
  )
}

function Quitar({
  modeloId,
  u,
  compartidos,
  onTrabajo,
}: {
  modeloId: number
  u: EstadoFrioModelo['ubicaciones'][number]
  compartidos: number
  onTrabajo: (id: number) => void
}) {
  const quitar = useMutation({
    mutationFn: () =>
      api<{ trabajo_id: number }>(`/frio/modelos/${modeloId}/quitar`, {
        method: 'POST',
        body: JSON.stringify({ ubicacion_id: u.ubicacion_id }),
      }),
    onSuccess: (r) => onTrabajo(r.trabajo_id),
  })
  return (
    <Stack gap="sm">
      <Text size="sm">
        Antes de quitar nada se lista {u.ubicacion} y se compara el MD5 de cada archivo del disco con su copia. Solo lo que
        coincide va a la papelera de la app; lo demás se queda.
      </Text>
      {compartidos > 0 && (
        <Text size="sm" c="dimmed">
          {plural(compartidos, 'archivo comparte', 'archivos comparten')} contenido con otro modelo: su espacio no se libera.
        </Text>
      )}
      {quitar.error && <Alert color="red">{quitar.error.message}</Alert>}
      <Button color="orange" loading={quitar.isPending} onClick={() => quitar.mutate()}>
        Quitar del disco (a la papelera)
      </Button>
    </Stack>
  )
}

function Traer({
  modeloId,
  e,
  u,
  onTrabajo,
}: {
  modeloId: number
  e: EstadoFrioModelo
  u: EstadoFrioModelo['ubicaciones'][number]
  onTrabajo: (id: number) => void
}) {
  const suyas = e.copias.filter((c) => c.ubicacion_id === u.ubicacion_id && c.estado === 'presente')
  const [elegidas, setElegidas] = useState<number[]>(suyas.filter((c) => !c.en_disco).map((c) => c.id))
  const traer = useMutation({
    mutationFn: () =>
      api<{ trabajo_id: number }>(`/frio/modelos/${modeloId}/traer`, {
        method: 'POST',
        body: JSON.stringify({ ubicacion_id: u.ubicacion_id, copia_ids: elegidas }),
      }),
    onSuccess: (r) => onTrabajo(r.trabajo_id),
  })
  const todas = elegidas.length === suyas.length
  return (
    <Stack gap="sm">
      <Text size="sm">
        Se bajan a una carpeta de entrada, se compara su MD5 con el registrado (y el CRC de lo que viene en un zip, que se
        extrae) y se ponen en la carpeta del modelo. Nada se sobrescribe: lo que choca se queda aparte.
      </Text>
      <Group gap="xs">
        <Button size="compact-xs" variant="subtle" onClick={() => setElegidas(todas ? [] : suyas.map((c) => c.id))}>
          {todas ? 'Ninguno' : 'Todos'}
        </Button>
        <Button size="compact-xs" variant="subtle" onClick={() => setElegidas(suyas.filter((c) => !c.en_disco).map((c) => c.id))}>
          Los que no están en el disco
        </Button>
      </Group>
      <ScrollArea.Autosize mah={300}>
        <Stack gap={2}>
          {suyas.map((c) => (
            <Checkbox
              key={c.id}
              size="xs"
              checked={elegidas.includes(c.id)}
              onChange={(ev) => setElegidas(ev.currentTarget.checked ? [...elegidas, c.id] : elegidas.filter((x) => x !== c.id))}
              label={
                <Text size="xs" style={{ wordBreak: 'break-all' }}>
                  {c.ruta} · {tamano(c.bytes)}
                  {c.en_disco ? ' · ya está en el disco' : ''}
                </Text>
              }
            />
          ))}
        </Stack>
      </ScrollArea.Autosize>
      {traer.error && <Alert color="red">{traer.error.message}</Alert>}
      <Button disabled={elegidas.length === 0} loading={traer.isPending} onClick={() => traer.mutate()}>
        Traer {plural(elegidas.length, 'archivo', 'archivos')}
      </Button>
    </Stack>
  )
}

/** En la ficha de un modelo: dónde están sus archivos (disco y frío) y mandar, quitar o traer. */
export default function FrioDelModelo({ modeloId }: { modeloId: number }) {
  const refrescar = useRefrescarFrio()
  const [accion, setAccion] = useState<{ tipo: 'mandar' } | { tipo: 'quitar' | 'traer'; ubicacion: number } | null>(null)
  const [trabajo, setTrabajo] = useState<number | null>(null)
  const [verCopias, setVerCopias] = useState(false)
  const q = useQuery({ queryKey: ['frio-modelo', modeloId], queryFn: () => api<EstadoFrioModelo>(`/frio/modelos/${modeloId}`) })
  const t = useTrabajo(trabajo)
  const estado = t.data?.estado
  useEffect(() => {
    if (estado && estado !== 'en_curso') refrescar()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [estado])
  if (!q.data) return null
  const e = q.data
  const ef = ESTADO_FRIO[e.estado]
  const ubicacion = accion && 'ubicacion' in accion ? e.ubicaciones.find((u) => u.ubicacion_id === accion.ubicacion) : undefined
  const empezar = (id: number) => {
    setAccion(null)
    setTrabajo(id)
  }
  const ultima = e.transferencias[0]
  return (
    <Card withBorder>
      <Group justify="space-between" align="flex-start">
        <div>
          <Group gap="xs">
            <Text fw={500}>Almacenamiento frío</Text>
            <Badge color={ef.color} variant="light" title={ef.ayuda}>
              {ef.texto}
            </Badge>
          </Group>
          <Text size="sm" c="dimmed">
            En el disco: {plural(e.disco.archivos, 'archivo', 'archivos')} ({tamano(e.disco.bytes)})
          </Text>
        </div>
        {e.disco.archivos > 0 && e.destinos.length > 0 && (
          <Button size="xs" variant="light" disabled={estado === 'en_curso'} onClick={() => setAccion({ tipo: 'mandar' })}>
            Mandar a frío
          </Button>
        )}
      </Group>
      {e.ubicaciones.map((u) => (
        <Group key={u.ubicacion_id} justify="space-between" mt="xs" wrap="wrap">
          <div>
            <Text size="sm">
              <b>{u.ubicacion}</b>: {plural(u.copias, 'copia', 'copias')} ({tamano(u.bytes)})
            </Text>
            <Text size="xs" c="dimmed">
              {n(u.verificadas)} verificadas por MD5
              {u.comprimidos ? ` · ${plural(u.comprimidos, 'comprimido', 'comprimidos')}` : ''}
              {u.solo_en_frio ? ` · ${n(u.solo_en_frio)} no están en el disco` : ' · todo está también en el disco'}
            </Text>
          </div>
          <Group gap="xs">
            {e.disco.archivos > 0 && u.verificadas > 0 && (
              <Button
                size="compact-xs"
                variant="subtle"
                color="orange"
                disabled={estado === 'en_curso'}
                onClick={() => setAccion({ tipo: 'quitar', ubicacion: u.ubicacion_id })}
              >
                Quitar del disco
              </Button>
            )}
            <Button
              size="compact-xs"
              variant="subtle"
              disabled={estado === 'en_curso'}
              onClick={() => setAccion({ tipo: 'traer', ubicacion: u.ubicacion_id })}
            >
              Traer
            </Button>
          </Group>
        </Group>
      ))}
      {estado === 'en_curso' && (
        <Stack gap={4} mt="sm">
          <Progress value={100} animated striped />
          <Text size="xs" c="dimmed">
            {t.data?.detalle}… {t.data?.progreso ? tamano(t.data.progreso) : ''} Puedes seguir usando la app.
          </Text>
        </Stack>
      )}
      {estado === 'fallo' && (
        <Alert color="red" mt="sm">
          {t.data?.detalle}
        </Alert>
      )}
      {ultima && (
        <Stack gap={2} mt="sm">
          <Text size="xs" c="dimmed">
            Último: {ultima.direccion === 'subir' ? 'mandado a' : 'traído de'} {ultima.ubicacion} el {cuando(ultima.creada)} ·{' '}
            {ultima.estado === 'terminada' ? 'sin problemas' : ultima.estado === 'en_curso' ? 'en curso' : ultima.estado.replace('_', ' ')}
          </Text>
          <ResumenResultado r={ultima.resultado} />
        </Stack>
      )}
      {e.papelera.length > 0 && (
        <Text size="xs" mt="xs">
          En la papelera: {e.papelera.map((l) => `${plural(l.archivos, 'archivo', 'archivos')} (${tamano(l.bytes)})`).join(', ')} ·{' '}
          <Anchor component={Link} to="/frio?ver=papelera" size="xs">
            ver la papelera
          </Anchor>
        </Text>
      )}
      {e.copias.length > 0 && (
        <>
          <Anchor size="xs" mt="xs" onClick={() => setVerCopias(!verCopias)}>
            {verCopias ? 'Ocultar' : 'Ver'} las {n(e.copias.length)} copias
          </Anchor>
          {verCopias && (
            <ScrollArea.Autosize mah={320} mt="xs">
              <Table>
                <Table.Tbody>
                  {e.copias.map((c) => (
                    <Table.Tr key={c.id} style={{ opacity: c.estado === 'falta' ? 0.5 : 1 }}>
                      <Table.Td>
                        <Text size="xs" style={{ wordBreak: 'break-all' }}>
                          {c.ruta}
                        </Text>
                      </Table.Td>
                      <Table.Td>
                        <Text size="xs" c="dimmed">
                          {c.estado === 'falta' ? 'ya no está' : c.verificado ? 'verificada' : 'sin verificar'}
                          {c.en_disco ? ' · en el disco' : ''}
                        </Text>
                      </Table.Td>
                      <Table.Td ta="right">
                        <Text size="xs">{tamano(c.bytes)}</Text>
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </ScrollArea.Autosize>
          )}
        </>
      )}
      <Modal
        opened={accion !== null}
        onClose={() => setAccion(null)}
        title={
          accion?.tipo === 'mandar'
            ? 'Mandar a frío'
            : accion?.tipo === 'quitar'
              ? `Quitar del disco (la copia está en ${ubicacion?.ubicacion})`
              : `Traer de ${ubicacion?.ubicacion}`
        }
        size="lg"
      >
        {accion?.tipo === 'mandar' && <Mandar modeloId={modeloId} e={e} onTrabajo={empezar} />}
        {accion?.tipo === 'quitar' && ubicacion && (
          <Quitar modeloId={modeloId} u={ubicacion} compartidos={e.disco.compartidos} onTrabajo={empezar} />
        )}
        {accion?.tipo === 'traer' && ubicacion && <Traer modeloId={modeloId} e={e} u={ubicacion} onTrabajo={empezar} />}
      </Modal>
    </Card>
  )
}
