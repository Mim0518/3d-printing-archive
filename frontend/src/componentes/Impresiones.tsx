import {
  Alert,
  Anchor,
  Autocomplete,
  Badge,
  Button,
  Card,
  Checkbox,
  Group,
  Loader,
  Modal,
  NumberInput,
  Pagination,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Table,
  Text,
  TextInput,
  Textarea,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { corto, cuando, dinero, duracion, n, plural, titulo } from '../formato'
import { ACABADO, RESULTADO, aNumero, nombreMaterial, useRefrescarExistencias, useTaller } from '../opciones'
import type { Acabado, Calculo, Desglose, Impresion, ListaImpresiones, PropuestaImpresion, Resultado } from '../tipos'

/** «Material $16.00 · luz $0.36 · limpieza $10.00 · fallas $2.64 = $29.00», y lo que falta para contarlo todo. */
export function VerDesglose({ d, costoUnitario, piezas }: { d: Desglose; costoUnitario?: number | null; piezas?: number }) {
  const partes = [
    ['material', d.material],
    ['luz', d.luz],
    ['limpieza', d.limpieza],
    ['fallas', d.fallas],
  ].filter(([, v]) => v != null) as [string, number][]
  return (
    <Stack gap={2}>
      <Text size="sm">
        {partes.map(([k, v]) => `${k} ${dinero(v)}`).join(' · ')}
        {partes.length > 0 ? ' = ' : ''}
        <b>{dinero(d.total)}</b>
        {costoUnitario != null && piezas ? (
          <>
            {' '}
            · <b>{dinero(costoUnitario)}</b> por miniatura ({n(piezas)})
          </>
        ) : null}
      </Text>
      {d.faltan.length > 0 && (
        <Text size="xs" c="orange">
          No se cuenta {d.faltan.join(', ')}.
        </Text>
      )}
    </Stack>
  )
}

/** Fecha y hora local para un `<input type="datetime-local">`. */
function ahoraLocal(): string {
  const d = new Date()
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset())
  return d.toISOString().slice(0, 16)
}

interface Pieza {
  modelo_id: number
  nombre: string
  cantidad: number | string
  acabado: Acabado
  ubicacion: string
}

function Formulario({ p, onListo }: { p: PropuestaImpresion; onListo: () => void }) {
  const { impresoras, materiales, perfiles } = useTaller()
  const refrescar = useRefrescarExistencias()
  const [fecha, setFecha] = useState(ahoraLocal)
  const [impresora, setImpresora] = useState(p.impresora_id ? String(p.impresora_id) : null)
  const [perfil, setPerfil] = useState(p.perfil_id ? String(p.perfil_id) : null)
  const [material, setMaterial] = useState(p.material_id ? String(p.material_id) : null)
  const [resultado, setResultado] = useState<Resultado>('bien')
  const [consumo, setConsumo] = useState<number | string>(p.consumo ?? '')
  const [unidad, setUnidad] = useState<'ml' | 'g'>(p.unidad)
  const [minutos, setMinutos] = useState<number | string>(p.tiempo_s ? Math.round(p.tiempo_s / 60) : '')
  const [notas, setNotas] = useState('')
  const [descontar, setDescontar] = useState(true)
  const [piezas, setPiezas] = useState<Pieza[]>(
    p.piezas.map((x) => ({
      modelo_id: x.modelo_id,
      nombre: titulo(x),
      cantidad: x.estado === 'confirmado' ? (x.cantidad ?? 0) : 0,
      acabado: 'sin_pintar',
      ubicacion: p.ubicaciones[0] ?? '',
    })),
  )
  const imp = impresoras.find((i) => String(i.id) === impresora)
  const mat = materiales.find((m) => String(m.id) === material)
  const tipo = imp ? (imp.tecnologia === 'fdm' ? 'filamento' : 'resina') : null
  const buenas = resultado === 'fallida' ? 0 : piezas.reduce((s, x) => s + (aNumero(x.cantidad) ?? 0), 0)
  const datos = {
    impresora_id: impresora ? Number(impresora) : null,
    material_id: material ? Number(material) : null,
    consumo: aNumero(consumo),
    unidad,
    tiempo_s: aNumero(minutos) != null ? Math.round(aNumero(minutos)! * 60) : null,
    piezas: buenas,
  }
  const [datosCalculo] = useDebouncedValue(datos, 300)
  const calculo = useQuery({
    queryKey: ['calculo', datosCalculo],
    queryFn: () => api<Calculo>('/impresiones/calcular', { method: 'POST', body: JSON.stringify(datosCalculo) }),
  })
  const comoSeLeyo = p.consumo != null && aNumero(consumo) === p.consumo && unidad === p.unidad
  const consumoDelLaminado = comoSeLeyo && p.consumo_origen === 'laminado'
  const guardar = useMutation({
    mutationFn: () =>
      api('/impresiones', {
        method: 'POST',
        body: JSON.stringify({
          placa_id: p.placa_id,
          impresora_id: datos.impresora_id,
          material_id: datos.material_id,
          perfil_id: perfil ? Number(perfil) : null,
          fecha: Math.floor(new Date(fecha).getTime() / 1000),
          resultado,
          consumo: datos.consumo,
          unidad,
          consumo_origen: consumoDelLaminado ? 'laminado' : 'capturado',
          tiempo_s: datos.tiempo_s,
          piezas:
            resultado === 'fallida'
              ? []
              : piezas.map((x) => ({
                  modelo_id: x.modelo_id,
                  cantidad: aNumero(x.cantidad) ?? 0,
                  acabado: x.acabado,
                  ubicacion: x.ubicacion,
                })),
          notas,
          descontar_material: descontar,
        }),
      }),
    onSuccess: () => {
      refrescar()
      onListo()
    },
  })
  const poner = (i: number, cambios: Partial<Pieza>) => setPiezas(piezas.map((x, k) => (k === i ? { ...x, ...cambios } : x)))

  return (
    <Stack gap="sm">
      <SimpleGrid cols={{ base: 1, sm: 2 }}>
        <TextInput label="Cuándo" type="datetime-local" value={fecha} onChange={(e) => setFecha(e.currentTarget.value)} />
        <div>
          <Text size="sm" fw={500} mb={4}>
            Cómo salió
          </Text>
          <SegmentedControl
            size="xs"
            value={resultado}
            onChange={(v) => setResultado(v as Resultado)}
            data={[
              { value: 'bien', label: 'Bien' },
              { value: 'parcial', label: 'Algunas fallaron' },
              { value: 'fallida', label: 'Falló toda' },
            ]}
          />
        </div>
      </SimpleGrid>
      <SimpleGrid cols={{ base: 1, sm: 3 }}>
        <Select
          label="Impresora"
          placeholder="Sin elegir"
          clearable
          data={impresoras.map((i) => ({ value: String(i.id), label: i.nombre }))}
          value={impresora}
          onChange={(v) => {
            setImpresora(v)
            const x = perfiles.find((y) => String(y.id) === perfil)
            if (x && String(x.impresora_id) !== v) setPerfil(null)
            // el material de la otra tecnología ya no sirve
            const otra = impresoras.find((i) => String(i.id) === v)
            if (otra && mat && mat.tipo !== (otra.tecnologia === 'fdm' ? 'filamento' : 'resina')) setMaterial(null)
          }}
        />
        <Select
          label="Perfil"
          placeholder="Sin elegir"
          clearable
          data={perfiles
            .filter((x) => !impresora || String(x.impresora_id) === impresora)
            .map((x) => ({ value: String(x.id), label: x.nombre }))}
          value={perfil}
          onChange={(v) => {
            setPerfil(v)
            const x = perfiles.find((y) => String(y.id) === v)
            if (x && !impresora) setImpresora(String(x.impresora_id))
            // el material del perfil, salvo que ya se haya elegido otro que sirva
            const deSuTipo = mat && x && mat.tipo === (x.tecnologia === 'fdm' ? 'filamento' : 'resina')
            if (x?.material_id && !deSuTipo) setMaterial(String(x.material_id))
          }}
        />
        <Select
          label="Material"
          placeholder="Sin elegir"
          clearable
          data={materiales.filter((m) => !tipo || m.tipo === tipo).map((m) => ({ value: String(m.id), label: nombreMaterial(m) }))}
          value={material}
          onChange={setMaterial}
        />
      </SimpleGrid>
      <SimpleGrid cols={{ base: 1, sm: 2 }}>
        <Group gap="xs" align="flex-end" wrap="nowrap">
          <NumberInput
            label="Material que gastó"
            description={
              consumoDelLaminado
                ? `Según ${p.laminado?.nombre ?? 'el laminado'}`
                : comoSeLeyo && p.consumo_origen === 'anterior'
                  ? 'Como la última vez'
                  : undefined
            }
            min={0}
            decimalScale={2}
            value={consumo}
            onChange={setConsumo}
            style={{ flex: 1 }}
          />
          <SegmentedControl size="xs" value={unidad} onChange={(v) => setUnidad(v as 'ml' | 'g')} data={['ml', 'g']} mb={2} />
        </Group>
        <NumberInput
          label="Tiempo de impresión"
          suffix=" min"
          min={0}
          allowDecimal={false}
          description={aNumero(minutos) ? duracion(Math.round(aNumero(minutos)! * 60)) : undefined}
          value={minutos}
          onChange={setMinutos}
        />
      </SimpleGrid>
      {mat?.queda != null && (
        <Checkbox
          label={`Descontarlo de lo que queda de ${nombreMaterial(mat)} (${corto(mat.queda)} ${mat.tipo === 'resina' ? 'ml' : 'g'})`}
          checked={descontar}
          onChange={(e) => setDescontar(e.currentTarget.checked)}
        />
      )}
      {resultado !== 'fallida' && (
        <Card withBorder padding="xs">
          <Text size="sm" fw={500}>
            Miniaturas que salieron bien
          </Text>
          <Text size="xs" c="dimmed" mb="xs">
            Entran a las existencias. Las que salieron mal no se cuentan.
          </Text>
          {piezas.length === 0 && (
            <Text size="sm" c="orange">
              La placa no tiene modelos: confírmalos en la placa para que sus miniaturas entren a las existencias.
            </Text>
          )}
          <Stack gap="xs">
            {piezas.map((x, i) => (
              <Group key={x.modelo_id} gap="xs" align="flex-end" wrap="wrap">
                <Text size="sm" style={{ flex: '1 1 160px' }}>
                  {x.nombre}
                </Text>
                <NumberInput
                  label={i === 0 ? 'Cuántas' : undefined}
                  min={0}
                  max={10000}
                  allowDecimal={false}
                  value={x.cantidad}
                  onChange={(v) => poner(i, { cantidad: v })}
                  w={90}
                />
                <Select
                  label={i === 0 ? 'Acabado' : undefined}
                  data={Object.entries(ACABADO).map(([value, a]) => ({ value, label: a.texto }))}
                  value={x.acabado}
                  allowDeselect={false}
                  onChange={(v) => v && poner(i, { acabado: v as Acabado })}
                  w={160}
                />
                <Autocomplete
                  label={i === 0 ? 'Dónde se guardan' : undefined}
                  placeholder="Caja 3…"
                  data={p.ubicaciones}
                  value={x.ubicacion}
                  onChange={(v) => poner(i, { ubicacion: v })}
                  w={160}
                />
              </Group>
            ))}
          </Stack>
        </Card>
      )}
      <Textarea label="Notas" placeholder="Qué falló, qué ajustaste…" autosize minRows={1} value={notas} onChange={(e) => setNotas(e.currentTarget.value)} />
      <Card withBorder padding="xs" bg="var(--mantine-color-default-hover)">
        <Text size="sm" fw={500}>
          Costo
        </Text>
        {calculo.data ? (
          <>
            <VerDesglose d={calculo.data.desglose} costoUnitario={calculo.data.costo_unitario} piezas={buenas} />
            {calculo.data.precio.sugerido != null && (
              <Text size="sm">
                Precio sugerido sin pintar: <b>{dinero(calculo.data.precio.sugerido)}</b> por miniatura
              </Text>
            )}
          </>
        ) : (
          <Loader size="xs" />
        )}
      </Card>
      {guardar.error && <Alert color="red">{guardar.error.message}</Alert>}
      <Group>
        <Button loading={guardar.isPending} onClick={() => guardar.mutate()}>
          Registrar impresión
        </Button>
        <Anchor component={Link} to="/existencias?ver=costos" size="sm">
          Ajustes de costos
        </Anchor>
      </Group>
    </Stack>
  )
}

/** Ventana para registrar que se imprimió una placa. */
export function RegistrarImpresion({ placaId, abierta, onClose }: { placaId: number; abierta: boolean; onClose: () => void }) {
  const q = useQuery({
    queryKey: ['propuesta-impresion', placaId],
    queryFn: () => api<PropuestaImpresion>(`/placas/${placaId}/impresion`),
    enabled: abierta,
  })
  return (
    <Modal opened={abierta} onClose={onClose} title="Registrar una impresión" size="xl">
      {q.isPending && <Loader />}
      {q.error && <Alert color="red">{q.error.message}</Alert>}
      {q.data && abierta && <Formulario key={q.dataUpdatedAt} p={q.data} onListo={onClose} />}
    </Modal>
  )
}

function Deshacer({ id }: { id: number }) {
  const refrescar = useRefrescarExistencias()
  const [seguro, setSeguro] = useState(false)
  const borrar = useMutation({
    mutationFn: () => api(`/impresiones/${id}`, { method: 'DELETE' }),
    onSuccess: refrescar,
  })
  if (borrar.error) {
    return (
      <Text size="xs" c="red" maw={260}>
        {borrar.error.message}
      </Text>
    )
  }
  return seguro ? (
    <Group gap={4} wrap="nowrap">
      <Button size="compact-xs" color="red" loading={borrar.isPending} onClick={() => borrar.mutate()}>
        Sí, deshacer
      </Button>
      <Button size="compact-xs" variant="subtle" color="gray" onClick={() => setSeguro(false)}>
        No
      </Button>
    </Group>
  ) : (
    <Button
      size="compact-xs"
      variant="subtle"
      color="gray"
      title="Borra el registro, saca sus miniaturas de las existencias y devuelve el material"
      onClick={() => setSeguro(true)}
    >
      Deshacer
    </Button>
  )
}

function Fila({ x, conPlaca }: { x: Impresion; conPlaca: boolean }) {
  return (
    <Table.Tr>
      <Table.Td>
        <Text size="sm" style={{ whiteSpace: 'nowrap' }}>
          {cuando(x.fecha)}
        </Text>
        <Badge size="xs" color={RESULTADO[x.resultado].color} variant="light">
          {RESULTADO[x.resultado].texto}
        </Badge>
      </Table.Td>
      {conPlaca && (
        <Table.Td>
          {x.placa_id ? (
            <Anchor component={Link} to={`/placas/${x.placa_id}`} size="sm">
              {x.placa}
            </Anchor>
          ) : (
            <Text size="sm">{x.placa || '—'}</Text>
          )}
        </Table.Td>
      )}
      <Table.Td>
        <Text size="sm">{[x.impresora, x.material].filter(Boolean).join(' · ') || '—'}</Text>
        <Text size="xs" c="dimmed">
          {[
            x.perfil ? `${x.perfil}${x.perfil_version ? ` v${x.perfil_version}` : ''}` : '',
            x.consumo != null ? `${corto(x.consumo)} ${x.unidad}${x.consumo_origen === 'laminado' ? ' (laminado)' : ''}` : '',
            duracion(x.tiempo_s),
          ]
            .filter(Boolean)
            .join(' · ')}
        </Text>
        {x.notas && (
          <Text size="xs" fs="italic">
            {x.notas}
          </Text>
        )}
      </Table.Td>
      <Table.Td>
        {x.miniaturas.length === 0 ? (
          <Text size="xs" c="dimmed">
            ninguna
          </Text>
        ) : (
          x.miniaturas.map((m) => (
            <Text key={`${m.modelo_id}-${m.acabado}-${m.ubicacion}`} size="xs">
              {m.cantidad} × {titulo(m)}
              {m.ubicacion ? ` · ${m.ubicacion}` : ''}
              {m.acabado !== 'sin_pintar' ? ` · ${ACABADO[m.acabado].texto}` : ''}
            </Text>
          ))
        )}
      </Table.Td>
      <Table.Td ta="right">
        <Text size="sm">{dinero(x.costo_total)}</Text>
        {x.piezas > 0 && (
          <Text size="xs" c="dimmed">
            {dinero(x.costo_total / x.piezas)} c/u
          </Text>
        )}
      </Table.Td>
      <Table.Td ta="right">
        <Deshacer id={x.id} />
      </Table.Td>
    </Table.Tr>
  )
}

/** Historial de impresiones (todas, de una placa o de un modelo). */
export function ListaDeImpresiones({ filtro = '', conPlaca = true, porPagina = 25 }: { filtro?: string; conPlaca?: boolean; porPagina?: number }) {
  const [pagina, setPagina] = useState(1)
  const q = useQuery({
    queryKey: ['impresiones', filtro, pagina, porPagina],
    queryFn: () => api<ListaImpresiones>(`/impresiones?${filtro}&pagina=${pagina}&por_pagina=${porPagina}`),
  })
  if (q.isPending) return <Loader size="sm" />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const d = q.data
  if (d.total === 0)
    return (
      <Text size="sm" c="dimmed">
        Todavía no hay impresiones registradas.
      </Text>
    )
  const paginas = Math.ceil(d.total / porPagina)
  return (
    <Stack gap="xs">
      <Text size="xs" c="dimmed">
        {plural(d.total, 'impresión', 'impresiones')} · {plural(d.piezas, 'miniatura', 'miniaturas')} · {dinero(d.costo)} en total
      </Text>
      <Table.ScrollContainer minWidth={700}>
        <Table verticalSpacing={6}>
          <Table.Tbody>
            {d.impresiones.map((x) => (
              <Fila key={x.id} x={x} conPlaca={conPlaca} />
            ))}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
      {paginas > 1 && <Pagination size="sm" total={paginas} value={pagina} onChange={setPagina} />}
    </Stack>
  )
}

/** En la ficha de una placa: cuánto costaría imprimirla, registrar una impresión y las que ya se hicieron. */
export function ImpresionesDePlaca({ placaId }: { placaId: number }) {
  const [abierta, setAbierta] = useState(false)
  const q = useQuery({
    queryKey: ['propuesta-impresion', placaId],
    queryFn: () => api<PropuestaImpresion>(`/placas/${placaId}/impresion`),
  })
  const e = q.data?.estimado
  const piezas = e?.piezas ?? 0
  return (
    <Card withBorder>
      <Group justify="space-between" align="flex-start">
        <div>
          <Text fw={500}>Impresiones</Text>
          {e && (
            <Stack gap={0}>
              <Text size="xs" c="dimmed">
                Si la imprimes así:
              </Text>
              <VerDesglose d={e.desglose} costoUnitario={e.costo_unitario} piezas={piezas} />
              {e.precio.sugerido != null && (
                <Text size="sm">
                  Precio sugerido sin pintar: <b>{dinero(e.precio.sugerido)}</b>
                </Text>
              )}
            </Stack>
          )}
        </div>
        <Button size="xs" onClick={() => setAbierta(true)}>
          Registrar impresión
        </Button>
      </Group>
      <Stack mt="sm">
        <ListaDeImpresiones filtro={`placa_id=${placaId}`} conPlaca={false} porPagina={10} />
      </Stack>
      <RegistrarImpresion placaId={placaId} abierta={abierta} onClose={() => setAbierta(false)} />
    </Card>
  )
}
