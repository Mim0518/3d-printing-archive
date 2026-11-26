import {
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Progress,
  SimpleGrid,
  Stack,
  Table,
  Text,
  Title,
} from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { COLOR_TIPO, cuando, n, tamano } from '../formato'
import { useYo } from '../sesion'
import { useTrabajo } from '../trabajo'
import type { Resumen, ResumenMiniaturas, ResumenPlacas, Trabajo } from '../tipos'

function Miniaturas() {
  const qc = useQueryClient()
  const r = useQuery({ queryKey: ['miniaturas'], queryFn: () => api<ResumenMiniaturas>('/miniaturas/resumen') })
  const [propio, setPropio] = useState<number | null>(null)
  const generar = useMutation({
    mutationFn: () => api<{ trabajo_id: number }>('/miniaturas', { method: 'POST' }),
    onSuccess: (x) => setPropio(x.trabajo_id),
  })
  const t = useTrabajo(r.data?.en_curso ?? propio)
  const estado = t.data?.estado
  useEffect(() => {
    if (estado && estado !== 'en_curso') void qc.invalidateQueries({ queryKey: ['miniaturas'] })
  }, [estado, qc])
  const d = r.data
  if (!d) return null
  const enCurso = t.data?.estado === 'en_curso'
  const hechas = d.con_miniatura + d.errores
  return (
    <Card withBorder>
      <Group justify="space-between" align="flex-start">
        <div>
          <Text fw={500}>Miniaturas</Text>
          <Text size="sm" c="dimmed">
            {n(d.con_miniatura)} de {n(d.archivos)} mallas e imágenes
            {d.errores > 0 && ` · ${n(d.errores)} no se pudieron`}
          </Text>
        </div>
        {d.pendientes > 0 && (
          <Button variant="light" loading={generar.isPending || enCurso} onClick={() => generar.mutate()}>
            Generar las {n(d.pendientes)} que faltan
          </Button>
        )}
      </Group>
      <Progress value={d.archivos ? (hechas * 100) / d.archivos : 0} mt="sm" />
      {enCurso && (
        <Text size="sm" c="dimmed" mt={4}>
          Generando… {n(t.data?.progreso ?? 0)} de {n(d.pendientes)} en este trabajo. Puedes seguir usando la app.
        </Text>
      )}
      {generar.error && (
        <Alert color="red" variant="light" mt="sm">
          {generar.error.message}
        </Alert>
      )}
    </Card>
  )
}

function Placas() {
  const r = useQuery({ queryKey: ['placas-resumen'], queryFn: () => api<ResumenPlacas>('/placas/resumen') })
  const d = r.data
  if (!d || d.total === 0) return null
  return (
    <Card withBorder>
      <Group justify="space-between" align="flex-start">
        <div>
          <Text fw={500}>Placas y escenas</Text>
          <Text size="sm" c="dimmed">
            {n(d.total)} placas · {n(d.confirmadas)} con modelo confirmado · {n(d.con_laminado)} con laminado
          </Text>
        </div>
        {d.por_confirmar > 0 && (
          <Button variant="light" component={Link} to="/placas?revision=por_confirmar">
            Revisar las {n(d.por_confirmar)} por confirmar
          </Button>
        )}
      </Group>
      <Progress value={d.total ? ((d.confirmadas + d.sin_modelo) * 100) / d.total : 0} mt="sm" color="green" />
    </Card>
  )
}

function Dato({ titulo, valor, detalle }: { titulo: string; valor: string; detalle?: string }) {
  return (
    <Card withBorder padding="md">
      <Text size="xs" c="dimmed" tt="uppercase" fw={600}>
        {titulo}
      </Text>
      <Text size="xl" fw={700}>
        {valor}
      </Text>
      {detalle && (
        <Text size="xs" c="dimmed">
          {detalle}
        </Text>
      )}
    </Card>
  )
}

function Escaneo({ resumen }: { resumen: Resumen }) {
  const qc = useQueryClient()
  const iniciar = useMutation({
    mutationFn: () => api<{ trabajo_id: number }>('/escaneos', { method: 'POST' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['resumen'] }),
  })
  const id = resumen.escaneo_en_curso ?? iniciar.data?.trabajo_id ?? null
  const trabajo = useQuery({
    queryKey: ['trabajo', id],
    queryFn: () => api<Trabajo>(`/trabajos/${id}`),
    enabled: id !== null,
    refetchInterval: (q) => (q.state.data?.estado === 'en_curso' || !q.state.data ? 1000 : false),
  })
  const t = trabajo.data
  const enCurso = t?.estado === 'en_curso'
  const estado = t?.estado
  useEffect(() => {
    // al terminar el escaneo, el resumen (totales, último escaneo) cambia
    if (estado && estado !== 'en_curso') {
      void qc.invalidateQueries({ queryKey: ['resumen'] })
      void qc.invalidateQueries({ queryKey: ['modelos'] })
    }
  }, [estado, qc])

  return (
    <Card withBorder>
      <Group justify="space-between" align="flex-start">
        <div>
          <Text fw={500}>Escaneo del disco</Text>
          <Text size="sm" c="dimmed">
            Último: {cuando(resumen.ultimo_escaneo)}
          </Text>
        </div>
        <Button onClick={() => iniciar.mutate()} loading={iniciar.isPending || enCurso} variant="light">
          Escanear ahora
        </Button>
      </Group>
      {iniciar.error && (
        <Alert color="red" variant="light" mt="sm">
          {iniciar.error.message}
        </Alert>
      )}
      {enCurso && (
        <Stack gap={4} mt="sm">
          <Progress value={100} animated striped />
          <Text size="sm" c="dimmed">
            {n(t.progreso ?? 0)} archivos revisados…
          </Text>
        </Stack>
      )}
      {t && t.estado === 'terminado' && t.resultado && iniciar.isSuccess && (
        <Text size="sm" mt="sm">
          Listo: {n(t.resultado.archivos ?? 0)} archivos · {n(t.resultado.nuevos ?? 0)} nuevos ·{' '}
          {n(t.resultado.cambiados ?? 0)} cambiados · {n(t.resultado.faltan ?? 0)} ya no están
          {(t.resultado.errores?.length ?? 0) > 0 && ` · ${t.resultado.errores?.length} errores`}
        </Text>
      )}
      {t && t.estado === 'fallo' && (
        <Alert color="red" variant="light" mt="sm">
          {t.detalle}
        </Alert>
      )}
    </Card>
  )
}

export default function Inicio() {
  const { data: usuario } = useYo()
  const r = useQuery({ queryKey: ['resumen'], queryFn: () => api<Resumen>('/resumen') })

  if (r.isPending) return <Loader />
  if (r.error) return <Alert color="red">{r.error.message}</Alert>
  const d = r.data
  const totalFrio = d.frio.reduce((s, f) => s + f.bytes, 0)

  return (
    <Stack maw={1000}>
      <Title order={2}>Hola, {usuario?.nombre}</Title>
      <SimpleGrid cols={{ base: 2, md: 4 }}>
        <Dato
          titulo="Modelos"
          valor={n(d.modelos.total)}
          detalle={`${n(d.modelos.total - d.modelos.revisados)} sin revisar`}
        />
        <Dato titulo="Archivos" valor={n(d.archivos.total)} detalle={`${n(d.archivos.faltan)} ya no están`} />
        <Dato titulo="En el disco" valor={tamano(d.archivos.bytes)} detalle="sin contar enlaces dos veces" />
        <Dato
          titulo="En frío"
          valor={tamano(totalFrio)}
          detalle={`${n(d.frio.reduce((s, f) => s + f.copias, 0))} copias`}
        />
      </SimpleGrid>

      <Escaneo resumen={d} />
      <Miniaturas />
      <Placas />

      <SimpleGrid cols={{ base: 1, md: 2 }}>
        <Card withBorder>
          <Text fw={500} mb="xs">
            Por juego
          </Text>
          <Table>
            <Table.Tbody>
              {d.por_juego.map((j) => (
                <Table.Tr key={j.juego}>
                  <Table.Td>
                    <Anchor component={Link} to={`/catalogo?juego=${encodeURIComponent(j.juego)}`}>
                      {j.juego || '(sin juego)'}
                    </Anchor>
                  </Table.Td>
                  <Table.Td ta="right">{n(j.modelos)}</Table.Td>
                  <Table.Td ta="right">{tamano(j.bytes)}</Table.Td>
                  <Table.Td ta="right">
                    {j.con_frio > 0 && (
                      <Text size="xs" c="blue" style={{ whiteSpace: 'nowrap' }}>
                        {n(j.con_frio)} con frío
                      </Text>
                    )}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Card>

        <Card withBorder>
          <Text fw={500} mb="xs">
            Por tipo de archivo
          </Text>
          <Table>
            <Table.Tbody>
              {d.por_tipo.map((t) => (
                <Table.Tr key={t.tipo}>
                  <Table.Td>
                    <Badge variant="light" color={COLOR_TIPO[t.tipo] ?? 'gray'}>
                      {t.tipo}
                    </Badge>
                  </Table.Td>
                  <Table.Td ta="right">{n(t.archivos)}</Table.Td>
                  <Table.Td ta="right">{tamano(t.bytes)}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Card>

        <Card withBorder>
          <Text fw={500}>Almacenamiento frío</Text>
          <Text size="xs" c="dimmed" mb="xs">
            Lo que está guardado fuera del disco
          </Text>
          {d.frio.length === 0 ? (
            <Text size="sm" c="dimmed">
              Todavía no se importa la lista de Drive.
            </Text>
          ) : (
            <Table>
              <Table.Tbody>
                {d.frio.map((f) => (
                  <Table.Tr key={f.ubicacion}>
                    <Table.Td>
                      <Text size="sm">{f.ubicacion}</Text>
                      <Text size="xs" c="dimmed">
                        {f.remoto}
                      </Text>
                    </Table.Td>
                    <Table.Td ta="right">{n(f.copias)}</Table.Td>
                    <Table.Td ta="right">{tamano(f.bytes)}</Table.Td>
                    <Table.Td ta="right">
                      <Text size="xs" c="dimmed">
                        {n(f.ligadas)} con modelo
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>

        <Card withBorder>
          <Text fw={500}>Fuera del catálogo</Text>
          <Text size="xs" c="dimmed" mb="xs">
            Archivos del disco que no son de ningún modelo
          </Text>
          <Table>
            <Table.Tbody>
              {d.fuera_del_catalogo.map((f) => (
                <Table.Tr key={f.carpeta}>
                  <Table.Td>{f.carpeta}</Table.Td>
                  <Table.Td ta="right">{n(f.archivos)}</Table.Td>
                  <Table.Td ta="right">{tamano(f.bytes)}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Card>
      </SimpleGrid>
    </Stack>
  )
}
