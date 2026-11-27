import {
  Alert,
  Badge,
  Button,
  CloseButton,
  Group,
  Loader,
  Pagination,
  Select,
  SimpleGrid,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useSearchParams } from 'react-router'

import { api } from '../api'
import TarjetaPlaca from '../componentes/TarjetaPlaca'
import { n, titulo } from '../formato'
import { useOpciones, useTaller } from '../opciones'
import { useTrabajo } from '../trabajo'
import type { Ficha, ListaPlacas, ResumenPlacas } from '../tipos'

const POR_PAGINA = 48
const CLAVES = ['q', 'revision', 'estado', 'juego', 'impresora_id', 'favorita', 'laminado', 'modelo_id', 'orden', 'pagina'] as const

/** Resumen, confirmación de lo que viene de la carpeta y nueva detección. */
function Encabezado() {
  const qc = useQueryClient()
  const r = useQuery({ queryKey: ['placas-resumen'], queryFn: () => api<ResumenPlacas>('/placas/resumen') })
  const [trabajo, setTrabajo] = useState<number | null>(null)
  const t = useTrabajo(r.data?.en_curso?.id ?? trabajo)
  const refrescar = () => {
    void qc.invalidateQueries({ queryKey: ['placas'] })
    void qc.invalidateQueries({ queryKey: ['placas-resumen'] })
  }
  const detectar = useMutation({
    mutationFn: () => api<{ trabajo_id: number }>('/placas/detectar', { method: 'POST' }),
    onSuccess: (x) => setTrabajo(x.trabajo_id),
  })
  const confirmar = useMutation({
    mutationFn: () => api<{ confirmadas: number }>('/placas/confirmar', { method: 'POST' }),
    onSuccess: refrescar,
  })
  const vistas = useMutation({
    mutationFn: () =>
      api<{ trabajo_id: number }>('/miniaturas', { method: 'POST', body: JSON.stringify({ solo_placas: true }) }),
    onSuccess: (x) => setTrabajo(x.trabajo_id),
  })
  const estado = t.data?.estado
  useEffect(() => {
    if (estado && estado !== 'en_curso') refrescar()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [estado])
  const d = r.data
  const enCurso = estado === 'en_curso'

  return (
    <Stack gap="xs">
      <Group justify="space-between" align="flex-end">
        <div>
          <Title order={2}>Placas y escenas</Title>
          {d && (
            <Text c="dimmed" size="sm">
              {n(d.total)} placas · {n(d.confirmadas)} con modelo confirmado · {n(d.por_confirmar)} por confirmar ·{' '}
              {n(d.con_laminado)} con laminado
            </Text>
          )}
        </div>
        <Group gap="xs">
          {d && d.vistas_pendientes > 0 && (
            <Button
              size="xs"
              variant="light"
              loading={vistas.isPending || enCurso}
              onClick={() => vistas.mutate()}
              title="Saca la imagen que traen adentro las escenas de Lychee y los laminados (no renderiza mallas)"
            >
              Sacar {n(d.vistas_pendientes)} vistas previas
            </Button>
          )}
          <Button
            size="xs"
            variant="default"
            loading={detectar.isPending || enCurso}
            onClick={() => detectar.mutate()}
            title="Vuelve a leer las escenas y laminados nuevos o que cambiaron (también corre después de cada escaneo)"
          >
            Volver a detectar
          </Button>
        </Group>
      </Group>
      {enCurso && (
        <Text size="sm" c="dimmed">
          {t.data?.tipo === 'miniaturas' ? 'Sacando vistas previas' : 'Trabajando'}… {n(t.data?.progreso ?? 0)}
        </Text>
      )}
      {(detectar.error ?? vistas.error) && <Alert color="red">{(detectar.error ?? vistas.error)?.message}</Alert>}
      {t.data?.estado === 'fallo' && <Alert color="red">{t.data.detalle}</Alert>}
      {d && d.sugerencias_por_carpeta > 0 && (
        <Alert color="blue" variant="light" title={`${n(d.sugerencias_por_carpeta)} sugerencias por la carpeta`}>
          <Text size="sm">
            Son placas que están dentro de la carpeta de un modelo, así que casi seguro son de ese modelo. Revisa las que
            quieras y confirma el resto de una vez; lo que rechaces no se vuelve a sugerir.
          </Text>
          {confirmar.error && (
            <Text size="sm" c="red">
              {confirmar.error.message}
            </Text>
          )}
          <Button size="xs" mt="xs" loading={confirmar.isPending} onClick={() => confirmar.mutate()}>
            Confirmar las {n(d.sugerencias_por_carpeta)}
          </Button>
        </Alert>
      )}
    </Stack>
  )
}

export default function Placas() {
  const [params, setParams] = useSearchParams()
  const [texto, setTexto] = useState(params.get('q') ?? '')
  const [buscado] = useDebouncedValue(texto, 300)
  const { filtros } = useOpciones()
  const { impresoras } = useTaller()

  const poner = (cambios: Partial<Record<(typeof CLAVES)[number], string | null>>) => {
    const p = new URLSearchParams(params)
    for (const [k, v] of Object.entries(cambios)) {
      if (v) p.set(k, v)
      else p.delete(k)
    }
    if (!('pagina' in cambios)) p.delete('pagina')
    setParams(p, { replace: true })
  }

  useEffect(() => {
    if ((params.get('q') ?? '') !== buscado) poner({ q: buscado })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [buscado])

  const consulta = new URLSearchParams()
  for (const k of CLAVES) {
    const v = params.get(k)
    if (v) consulta.set(k, v)
  }
  consulta.set('por_pagina', String(POR_PAGINA))
  const lista = useQuery({
    queryKey: ['placas', consulta.toString()],
    queryFn: () => api<ListaPlacas>(`/placas?${consulta}`),
    placeholderData: keepPreviousData,
  })
  const modeloId = params.get('modelo_id')
  const modelo = useQuery({
    queryKey: ['modelo', modeloId],
    queryFn: () => api<Ficha>(`/modelos/${modeloId}`),
    enabled: !!modeloId,
  })
  const paginas = lista.data ? Math.max(1, Math.ceil(lista.data.total / POR_PAGINA)) : 1
  const solo = params.get('favorita') ? 'favoritas' : params.get('laminado') ? 'laminado' : null

  return (
    <Stack>
      <Encabezado />
      <TextInput
        placeholder="Buscar por nombre de la placa, carpeta o modelo…"
        value={texto}
        onChange={(e) => setTexto(e.currentTarget.value)}
      />
      <Group gap="xs" grow wrap="wrap">
        <Select
          placeholder="Revisión"
          clearable
          data={[
            { value: 'por_confirmar', label: 'Por confirmar' },
            { value: 'confirmadas', label: 'Con modelo confirmado' },
            { value: 'sin_sugerencia', label: 'Sin sugerencia' },
            { value: 'sin_modelo', label: 'Sin modelo (pruebas, calibración)' },
          ]}
          value={params.get('revision')}
          onChange={(v) => poner({ revision: v })}
          miw={170}
        />
        <Select
          placeholder="Estado"
          clearable
          data={[
            { value: 'sin_probar', label: 'Sin probar' },
            { value: 'probada', label: 'Probadas' },
            { value: 'fallida', label: 'Fallidas' },
          ]}
          value={params.get('estado')}
          onChange={(v) => poner({ estado: v })}
          miw={130}
        />
        <Select
          placeholder="Juego"
          clearable
          data={(filtros?.juegos ?? []).map((j) => j.nombre)}
          value={params.get('juego')}
          onChange={(v) => poner({ juego: v })}
          miw={150}
        />
        {impresoras.length > 0 && (
          <Select
            placeholder="Impresora"
            clearable
            data={impresoras.map((i) => ({ value: String(i.id), label: i.nombre }))}
            value={params.get('impresora_id')}
            onChange={(v) => poner({ impresora_id: v })}
            miw={170}
          />
        )}
        <Select
          placeholder="Mostrar"
          clearable
          data={[
            { value: 'laminado', label: 'Solo con laminado' },
            { value: 'favoritas', label: 'Solo favoritas' },
          ]}
          value={solo}
          onChange={(v) => poner({ laminado: v === 'laminado' ? 'si' : null, favorita: v === 'favoritas' ? 'si' : null })}
          miw={150}
        />
        <Select
          placeholder="Orden"
          data={[
            { value: 'nombre', label: 'Por carpeta' },
            { value: 'reciente', label: 'Más recientes' },
          ]}
          value={params.get('orden') ?? 'nombre'}
          onChange={(v) => poner({ orden: v === 'nombre' ? null : v })}
          miw={130}
        />
      </Group>
      {modeloId && (
        <Group gap="xs">
          <Badge
            size="lg"
            variant="light"
            rightSection={<CloseButton size="xs" aria-label="Quitar filtro" onClick={() => poner({ modelo_id: null })} />}
          >
            Del modelo: {modelo.data ? titulo(modelo.data) : `#${modeloId}`}
          </Badge>
        </Group>
      )}
      {lista.error && <Alert color="red">{lista.error.message}</Alert>}
      {lista.data && (
        <Text c="dimmed" size="sm">
          {n(lista.data.total)} placas
        </Text>
      )}
      {lista.isPending ? (
        <Loader />
      ) : (
        <SimpleGrid cols={{ base: 2, xs: 3, md: 4, lg: 6 }} spacing="sm" style={{ opacity: lista.isPlaceholderData ? 0.6 : 1 }}>
          {lista.data?.placas.map((p) => <TarjetaPlaca key={p.id} placa={p} />)}
        </SimpleGrid>
      )}
      {lista.data?.placas.length === 0 && (
        <Text c="dimmed" ta="center" py="xl">
          Nada coincide con la búsqueda.
        </Text>
      )}
      {paginas > 1 && (
        <Group justify="center">
          <Pagination
            total={paginas}
            value={Number(params.get('pagina') ?? 1)}
            onChange={(p) => poner({ pagina: p > 1 ? String(p) : null })}
          />
        </Group>
      )}
    </Stack>
  )
}
